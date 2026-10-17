//! Panneau « Réglages » : onglets Général et Limites.

use super::widgets::{caption, ghost_button, toggle};
use super::{CopyApp, SettingsTab};
use crate::history;
use crate::logging::LockExt;
use crate::settings;
use crate::theme::Palette;
use crate::{login_item, paste, storage};
use eframe::egui::{self, Color32, RichText, Stroke, Ui};
use std::sync::atomic::Ordering;

impl CopyApp {
    /// Contenu du panneau « Réglages ».
    pub(super) fn settings_ui(&mut self, ui: &mut Ui, p: &Palette) {
        ui.set_width(330.0);
        ui.spacing_mut().item_spacing.y = 4.0;
        ui.horizontal(|ui| {
            ui.label(RichText::new("Réglages").size(13.5).color(p.text).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                for (tab, label) in [
                    (SettingsTab::Limits, "Limites"),
                    (SettingsTab::General, "Général"),
                ] {
                    let active = self.settings_tab == tab;
                    let text = RichText::new(label).size(12.5).color(if active {
                        p.accent
                    } else {
                        p.text_muted
                    });
                    let button = egui::Button::new(text)
                        .fill(if active {
                            p.accent_soft
                        } else {
                            Color32::TRANSPARENT
                        })
                        .rounding(6.0);
                    if ui.add(button).clicked() {
                        self.settings_tab = tab;
                    }
                }
            });
        });
        ui.add_space(6.0);
        let changed = match self.settings_tab {
            SettingsTab::General => self.general_settings_ui(ui, p),
            SettingsTab::Limits => self.limits_ui(ui, p),
        };
        if changed {
            self.settings.save(&settings::default_path());
        }
    }

    /// Onglet « Limites » : on modifie un brouillon, appliqué sur demande
    /// (réduire une limite supprime des éléments).
    fn limits_ui(&mut self, ui: &mut Ui, p: &Palette) -> bool {
        let mut draft = self
            .limits_draft
            .take()
            .unwrap_or_else(|| self.settings.clone());
        let row = |ui: &mut Ui, label: &str, help: &str, add: &mut dyn FnMut(&mut Ui)| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(label).size(12.5).color(p.text));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Champs bien visibles : cadre, largeur minimale.
                    let v = ui.visuals_mut();
                    for w in [
                        &mut v.widgets.inactive,
                        &mut v.widgets.hovered,
                        &mut v.widgets.active,
                    ] {
                        w.bg_stroke = Stroke::new(1.0_f32, p.border);
                    }
                    v.widgets.inactive.bg_fill = p.bg;
                    v.widgets.inactive.weak_bg_fill = p.bg;
                    ui.spacing_mut().interact_size.x = 78.0;
                    add(ui)
                });
            });
            caption(ui, help, p);
            ui.add_space(6.0);
        };
        row(
            ui,
            "Nombre d'éléments",
            "Au-delà, les plus anciens sont supprimés.",
            &mut |ui| {
                ui.add(
                    egui::DragValue::new(&mut draft.max_items)
                        .range(settings::ITEMS_RANGE)
                        .speed(5.0),
                );
            },
        );
        row(
            ui,
            "Nombre d'images",
            "Comptées dans le total ; 0 = aucune image gardée.",
            &mut |ui| {
                ui.add(egui::DragValue::new(&mut draft.max_images).range(settings::IMAGES_RANGE));
            },
        );
        row(
            ui,
            "Taille d'un texte",
            "Un texte plus long est ignoré.",
            &mut |ui| {
                ui.add(
                    egui::DragValue::new(&mut draft.max_text_kb)
                        .range(settings::TEXT_KB_RANGE)
                        .speed(10.0)
                        .suffix(" Ko"),
                );
            },
        );
        row(
            ui,
            "Taille totale des textes",
            "Au-delà, les plus anciens sont supprimés.",
            &mut |ui| {
                ui.add(
                    egui::DragValue::new(&mut draft.max_total_mb)
                        .range(settings::TOTAL_MB_RANGE)
                        .speed(5.0)
                        .suffix(" Mo"),
                );
            },
        );
        row(
            ui,
            "Taille d'une image",
            "Une image plus grande est ignorée.",
            &mut |ui| {
                ui.add(
                    egui::DragValue::new(&mut draft.max_image_mp)
                        .range(settings::IMAGE_MP_RANGE)
                        .suffix(" Mpx"),
                );
            },
        );
        row(
            ui,
            "Conservation",
            "Les éléments plus anciens sont effacés automatiquement.",
            &mut |ui| {
                let current = settings::RETENTION_CHOICES
                    .iter()
                    .find(|(d, _)| *d == draft.retention_days)
                    .map(|(_, l)| l.to_string())
                    .unwrap_or_else(|| format!("{} jours", draft.retention_days));
                egui::ComboBox::from_id_salt("retention")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for (days, label) in settings::RETENTION_CHOICES {
                            ui.selectable_value(&mut draft.retention_days, days, label);
                        }
                    });
            },
        );

        let mut changed = false;
        let pending = draft != self.settings;
        ui.horizontal(|ui| {
            let defaults = self.settings.with_default_limits();
            if draft != defaults
                && ghost_button(ui, "Valeurs par défaut", p.text_muted, p).clicked()
            {
                draft = defaults;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if pending {
                    let apply = egui::Button::new(RichText::new("Appliquer").color(Color32::WHITE))
                        .fill(p.accent)
                        .rounding(7.0);
                    if ui.add(apply).clicked() {
                        self.settings = draft.clone();
                        let removed = self.apply_limits();
                        self.notify(if removed > 0 {
                            format!("Limites appliquées : {removed} élément(s) supprimé(s)")
                        } else {
                            "Limites appliquées".to_string()
                        });
                        changed = true;
                    }
                    if ghost_button(ui, "Annuler", p.text_muted, p).clicked() {
                        draft = self.settings.clone();
                    }
                }
            });
        });
        if draft != self.settings {
            let n = self
                .history
                .lock_safe()
                .would_remove(draft.limits(), history::now_secs());
            if n > 0 {
                ui.label(
                    RichText::new(format!("⚠ {n} élément(s) seront supprimés définitivement."))
                        .size(11.5)
                        .color(p.danger),
                );
            }
        }
        self.limits_draft = Some(draft);
        changed
    }

    /// Onglet « Général ». Renvoie vrai si un réglage a changé.
    fn general_settings_ui(&mut self, ui: &mut Ui, p: &Palette) -> bool {
        let mut changed = false;
        ui.horizontal(|ui| {
            if toggle(
                ui,
                &mut self.settings.auto_paste,
                "Coller automatiquement",
                p,
            )
            .changed()
            {
                changed = true;
                if self.settings.auto_paste && !paste::is_trusted() {
                    paste::request_trust();
                }
            }
        });
        caption(
            ui,
            "L'élément choisi est collé directement dans l'application d'où vous avez ouvert \
             l'historique (⌘⇧Z).",
            p,
        );
        if self.settings.auto_paste && !paste::is_trusted() {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new("⚠ Autorisation « Accessibilité » requise.")
                        .size(11.5)
                        .color(p.danger),
                );
                if ghost_button(ui, "Ouvrir les réglages", p.accent, p).clicked() {
                    paste::open_accessibility_settings();
                }
            });
        }
        ui.add_space(8.0);
        if !self.settings.auto_paste {
            ui.horizontal(|ui| {
                changed |= toggle(
                    ui,
                    &mut self.settings.hide_after_copy,
                    "Masquer après copie",
                    p,
                )
                .changed();
            });
            caption(
                ui,
                "La fenêtre se ferme dès qu'un élément est choisi : il reste à faire ⌘V.",
                p,
            );
            ui.add_space(8.0);
        }

        ui.horizontal(|ui| {
            if toggle(
                ui,
                &mut self.settings.save_images,
                "Enregistrer les images",
                p,
            )
            .changed()
            {
                changed = true;
                self.shared
                    .save_images
                    .store(self.settings.save_images, Ordering::Relaxed);
            }
        });
        caption(
            ui,
            "Captures d'écran et images copiées (50 au plus). Les fichiers copiés dans le \
             Finder (vidéos, sons, PDF…) sont toujours gardés, par leur emplacement.",
            p,
        );
        ui.add_space(8.0);

        let available = self.login_status != login_item::Status::Unavailable;
        let mut on = matches!(
            self.login_status,
            login_item::Status::Enabled | login_item::Status::RequiresApproval
        );
        let resp = ui
            .add_enabled_ui(available, |ui| {
                ui.horizontal(|ui| toggle(ui, &mut on, "Ouvrir au démarrage", p))
                    .inner
            })
            .inner;
        if available && resp.changed() {
            self.set_login_item(on);
        }
        caption(
            ui,
            if available {
                "Lance CopyQ-rs à l'ouverture de votre session."
            } else {
                "Disponible une fois l'app installée en tant qu'application (.app)."
            },
            p,
        );
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            if toggle(
                ui,
                &mut self.settings.save_passwords,
                "Enregistrer les mots de passe",
                p,
            )
            .changed()
            {
                changed = true;
                self.shared
                    .save_passwords
                    .store(self.settings.save_passwords, Ordering::Relaxed);
                log::info!(
                    "enregistrement des mots de passe : {}",
                    self.settings.save_passwords
                );
            }
        });
        caption(
            ui,
            "Copies marquées confidentielles par 1Password, Bitwarden, le trousseau… \
             Chiffrées sur le disque avec une clé gardée dans le trousseau macOS, \
             affichées masquées et exclues de la recherche.",
            p,
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if toggle(
                ui,
                &mut self.settings.detect_passwords,
                "Détecter les mots de passe",
                p,
            )
            .changed()
            {
                changed = true;
                self.shared
                    .detect_passwords
                    .store(self.settings.detect_passwords, Ordering::Relaxed);
                log::info!(
                    "détection des mots de passe : {}",
                    self.settings.detect_passwords
                );
            }
        });
        caption(
            ui,
            "Pour les extensions de navigateur (Dashlane, 1Password, Bitwarden…) qui ne \
             signalent pas leurs copies : un texte sans espace de 8 à 128 caractères mêlant \
             majuscules, minuscules et chiffres. ⌘L corrige une erreur sur un élément.",
            p,
        );
        if changed && self.settings.detect_passwords {
            self.reclassify();
        }
        if let Some(err) = storage::encryption_error() {
            ui.add(
                egui::Label::new(
                    RichText::new(format!(
                        "⚠ Trousseau inaccessible ({err}) : les mots de passe restent en mémoire, \
                         sans être enregistrés."
                    ))
                    .size(11.5)
                    .color(p.danger),
                )
                .wrap(),
            );
        }
        let stored = self.history.lock_safe().sensitive_count();
        if stored > 0 {
            ui.add_space(4.0);
            let label = if stored == 1 {
                "Effacer le mot de passe enregistré".to_string()
            } else {
                format!("Effacer les {stored} mots de passe enregistrés")
            };
            if ghost_button(ui, &label, p.danger, p).clicked() {
                let n = self.history.lock_safe().remove_sensitive();
                self.notify(format!("{n} mot(s) de passe effacé(s)"));
            }
        }

        changed
    }
}
