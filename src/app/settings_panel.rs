//! Panneau « Réglages » : onglets Général et Limites.

use super::widgets::{
    caption, column, ghost_button, hstack, hwrap, paragraph, toggle, trailing, wt,
};
use super::{CopyApp, SettingsTab};
use crate::history;
use crate::logging::LockExt;
use crate::settings;
use crate::theme::Palette;
use crate::{i18n, login_item, paste, storage};
use eframe::egui::{self, Color32, RichText, Stroke, Ui};
use std::sync::atomic::Ordering;

impl CopyApp {
    /// Contenu du panneau « Réglages » (aligné à droite en arabe).
    pub(super) fn settings_ui(&mut self, ui: &mut Ui, p: &Palette) {
        ui.set_width(330.0);
        ui.with_layout(column(), |ui| self.settings_body(ui, p));
    }

    fn settings_body(&mut self, ui: &mut Ui, p: &Palette) {
        ui.spacing_mut().item_spacing.y = 4.0;
        hstack(ui, |ui| {
            ui.label(wt(t!("settings.title"), 13.5, p.text));
            ui.with_layout(trailing(), |ui| {
                for (tab, label) in [
                    (SettingsTab::Limits, t!("settings.tab_limits")),
                    (SettingsTab::General, t!("settings.tab_general")),
                ] {
                    let active = self.settings_tab == tab;
                    let text = wt(label, 12.5, if active { p.accent } else { p.text_muted });
                    let button = egui::Button::new(text)
                        .fill(if active {
                            p.accent_soft
                        } else {
                            Color32::TRANSPARENT
                        })
                        .corner_radius(6.0);
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
            hstack(ui, |ui| {
                ui.label(wt(label, 12.5, p.text));
                ui.with_layout(trailing(), |ui| {
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
            t!("limits.items"),
            t!("limits.oldest_deleted"),
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
            t!("limits.images"),
            t!("limits.images_help"),
            &mut |ui| {
                ui.add(egui::DragValue::new(&mut draft.max_images).range(settings::IMAGES_RANGE));
            },
        );
        row(
            ui,
            t!("limits.text_size"),
            t!("limits.text_size_help"),
            &mut |ui| {
                ui.add(
                    egui::DragValue::new(&mut draft.max_text_kb)
                        .range(settings::TEXT_KB_RANGE)
                        .speed(10.0)
                        .suffix(t!("limits.unit_kb")),
                );
            },
        );
        row(
            ui,
            t!("limits.total_size"),
            t!("limits.oldest_deleted"),
            &mut |ui| {
                ui.add(
                    egui::DragValue::new(&mut draft.max_total_mb)
                        .range(settings::TOTAL_MB_RANGE)
                        .speed(5.0)
                        .suffix(t!("limits.unit_mb")),
                );
            },
        );
        row(
            ui,
            t!("limits.image_size"),
            t!("limits.image_size_help"),
            &mut |ui| {
                ui.add(
                    egui::DragValue::new(&mut draft.max_image_mp)
                        .range(settings::IMAGE_MP_RANGE)
                        .suffix(t!("limits.unit_mpx")),
                );
            },
        );
        row(
            ui,
            t!("limits.retention"),
            t!("limits.retention_help"),
            &mut |ui| {
                egui::ComboBox::from_id_salt("retention")
                    .selected_text(wt(
                        &settings::retention_label(draft.retention_days),
                        13.0,
                        p.text,
                    ))
                    .show_ui(ui, |ui| {
                        for days in settings::RETENTION_CHOICES {
                            let label = wt(&settings::retention_label(days), 13.0, p.text);
                            ui.selectable_value(&mut draft.retention_days, days, label);
                        }
                    });
            },
        );

        let mut changed = false;
        let pending = draft != self.settings;
        hstack(ui, |ui| {
            let defaults = self.settings.with_default_limits();
            if draft != defaults
                && ghost_button(ui, t!("limits.defaults"), p.text_muted, p).clicked()
            {
                draft = defaults;
            }
            ui.with_layout(trailing(), |ui| {
                if pending {
                    let apply = egui::Button::new(wt(t!("limits.apply"), 13.0, Color32::WHITE))
                        .fill(p.accent)
                        .corner_radius(7.0);
                    if ui.add(apply).clicked() {
                        self.settings = draft.clone();
                        let removed = self.apply_limits();
                        self.notify(if removed > 0 {
                            t!("toast.limits_applied_removed", n = removed)
                        } else {
                            t!("toast.limits_applied").to_string()
                        });
                        changed = true;
                    }
                    if ghost_button(ui, t!("footer.cancel"), p.text_muted, p).clicked() {
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
                ui.label(wt(&t!("limits.will_remove", n = n), 11.5, p.danger));
            }
        }
        self.limits_draft = Some(draft);
        changed
    }

    /// Onglet « Général ». Renvoie vrai si un réglage a changé.
    fn general_settings_ui(&mut self, ui: &mut Ui, p: &Palette) -> bool {
        let mut changed = self.language_ui(ui, p);
        ui.add_space(8.0);
        hstack(ui, |ui| {
            if toggle(
                ui,
                &mut self.settings.auto_paste,
                t!("general.auto_paste"),
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
        caption(ui, t!("general.auto_paste_help"), p);
        if self.settings.auto_paste && !paste::is_trusted() {
            hwrap(ui, |ui| {
                ui.label(wt(t!("general.accessibility_needed"), 11.5, p.danger));
                if ghost_button(ui, t!("general.open_settings"), p.accent, p).clicked() {
                    paste::open_accessibility_settings();
                }
            });
        }
        ui.add_space(8.0);
        if !self.settings.auto_paste {
            hstack(ui, |ui| {
                changed |= toggle(
                    ui,
                    &mut self.settings.hide_after_copy,
                    t!("general.hide_after_copy"),
                    p,
                )
                .changed();
            });
            caption(ui, t!("general.hide_after_copy_help"), p);
            ui.add_space(8.0);
        }

        hstack(ui, |ui| {
            if toggle(
                ui,
                &mut self.settings.save_images,
                t!("general.save_images"),
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
        caption(ui, t!("general.save_images_help"), p);
        ui.add_space(8.0);

        let available = self.login_status != login_item::Status::Unavailable;
        let mut on = matches!(
            self.login_status,
            login_item::Status::Enabled | login_item::Status::RequiresApproval
        );
        let resp = ui
            .add_enabled_ui(available, |ui| {
                hstack(ui, |ui| toggle(ui, &mut on, t!("general.open_at_login"), p)).inner
            })
            .inner;
        if available && resp.changed() {
            self.set_login_item(on);
        }
        caption(
            ui,
            if available {
                t!("general.open_at_login_help")
            } else {
                t!("general.open_at_login_unavailable")
            },
            p,
        );
        ui.add_space(8.0);

        hstack(ui, |ui| {
            if toggle(
                ui,
                &mut self.settings.save_passwords,
                t!("general.save_passwords"),
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
        caption(ui, t!("general.save_passwords_help"), p);
        ui.add_space(8.0);
        hstack(ui, |ui| {
            if toggle(
                ui,
                &mut self.settings.detect_passwords,
                t!("general.detect_passwords"),
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
        caption(ui, t!("general.detect_passwords_help"), p);
        if changed && self.settings.detect_passwords {
            self.reclassify();
        }
        if let Some(err) = storage::encryption_error() {
            paragraph(
                ui,
                &t!("general.keychain_error", error = err),
                11.5,
                p.danger,
            );
        }
        let stored = self.history.lock_safe().sensitive_count();
        if stored > 0 {
            ui.add_space(4.0);
            let label = if stored == 1 {
                t!("general.erase_password").to_string()
            } else {
                t!("general.erase_passwords", n = stored)
            };
            if ghost_button(ui, &label, p.danger, p).clicked() {
                let n = self.history.lock_safe().remove_sensitive();
                self.notify(t!("toast.passwords_erased", n = n));
            }
        }

        changed
    }

    /// Choix de la langue : « Système (…) » ou l'une des langues traduites.
    fn language_ui(&mut self, ui: &mut Ui, p: &Palette) -> bool {
        let system = i18n::detect();
        let label = |code: &str| match i18n::Lang::from_code(code) {
            Some(lang) => lang.native_name().to_string(),
            None => t!("settings.language_auto", name = system.native_name()),
        };
        let before = self.settings.language.clone();
        hstack(ui, |ui| {
            ui.label(wt(t!("settings.language"), 12.5, p.text_muted));
            ui.with_layout(trailing(), |ui| {
                egui::ComboBox::from_id_salt("language")
                    .selected_text(wt(&label(&self.settings.language), 13.0, p.text))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.settings.language,
                            settings::AUTO_LANGUAGE.to_string(),
                            wt(&label(settings::AUTO_LANGUAGE), 13.0, p.text),
                        );
                        for lang in i18n::ALL {
                            ui.selectable_value(
                                &mut self.settings.language,
                                lang.code().to_string(),
                                wt(lang.native_name(), 13.0, p.text),
                            );
                        }
                    });
            });
        });
        let changed = self.settings.language != before;
        if changed {
            let ctx = ui.ctx().clone();
            self.apply_language(&ctx);
        }
        changed
    }
}
