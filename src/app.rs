//! Interface egui : recherche, liste de l'historique, navigation clavier.

use crate::history::{self, History};
use crate::logging::LockExt;
use crate::settings::{self, Settings};
use crate::theme::{self, Palette};
use crate::{clipboard, hotkey, icons, login_item, platform, storage, tray};
use eframe::egui::{
    self, text::LayoutJob, text::TextWrapping, Align2, Color32, FontId, Frame, Margin, Rect,
    RichText, Rounding, Sense, Stroke, Ui,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const PREVIEW_CHARS: usize = 160;
const TOOLTIP_CHARS: usize = 1_500;
const ROW_HEIGHT: f32 = 54.0;
const SETTINGS_POPUP: &str = "settings_popup";
/// Hauteur réservée en haut pour les boutons de fenêtre macOS (barre de titre transparente).
const TITLEBAR_HEIGHT: f32 = 28.0;

/// Nature du contenu, pour choisir l'icône de la ligne.
#[derive(Clone, Copy)]
enum Kind {
    Link,
    Path,
    Email,
    Multiline,
    Text,
    Secret,
}

impl Kind {
    fn detect(text: &str) -> Self {
        let t = text.trim();
        let single_word = !t.contains(char::is_whitespace);
        if single_word && (t.starts_with("http://") || t.starts_with("https://")) {
            Kind::Link
        } else if single_word && (t.starts_with('/') || t.starts_with("~/")) {
            Kind::Path
        } else if single_word && t.contains('@') && t.contains('.') {
            Kind::Email
        } else if t.contains('\n') {
            Kind::Multiline
        } else {
            Kind::Text
        }
    }

    fn paint(self, painter: &egui::Painter, center: egui::Pos2, color: Color32) {
        match self {
            Kind::Link => icons::globe(painter, center, color),
            Kind::Path => icons::folder(painter, center, color),
            Kind::Email => icons::envelope(painter, center, color),
            Kind::Multiline => icons::lines(painter, center, color),
            Kind::Secret => icons::lock(painter, center, color),
            Kind::Text => {
                painter.text(
                    center,
                    Align2::CENTER_CENTER,
                    "Aa",
                    FontId::proportional(12.5),
                    color,
                );
            }
        }
    }
}

/// Ligne affichée, calculée une fois puis mise en cache.
struct Row {
    index: usize,
    title: String,
    kind: Kind,
    copied_at: u64,
    lines: usize,
    chars: usize,
    sensitive: bool,
}

struct Toast {
    message: String,
    at: Instant,
    duration: Duration,
    error: bool,
}

pub struct CopyApp {
    history: Arc<Mutex<History>>,
    paused: Arc<AtomicBool>,
    clipboard: Option<arboard::Clipboard>,
    query: String,
    selected: usize,
    scroll_to_selected: bool,
    settings: Settings,
    /// Partagés avec la surveillance du presse-papiers.
    save_passwords: Arc<AtomicBool>,
    detect_passwords: Arc<AtomicBool>,
    confirm_clear: bool,
    open_settings_on_start: bool,
    login_status: login_item::Status,
    toast: Option<Toast>,
    /// (version de l'historique, requête) → lignes.
    cache: Option<(u64, String, Arc<Vec<Row>>)>,
    _tray: Option<tray_icon::TrayIcon>,
    _hotkey: Option<global_hotkey::GlobalHotKeyManager>,
}

impl CopyApp {
    pub fn new(cc: &eframe::CreationContext<'_>, history: Arc<Mutex<History>>) -> Self {
        theme::install(&cc.egui_ctx);

        let settings = Settings::load(&settings::default_path());
        log::info!("réglages : {settings:?}");
        let paused = Arc::new(AtomicBool::new(false));
        let save_passwords = Arc::new(AtomicBool::new(settings.save_passwords));
        let detect_passwords = Arc::new(AtomicBool::new(settings.detect_passwords));
        clipboard::spawn_watcher(
            history.clone(),
            paused.clone(),
            save_passwords.clone(),
            detect_passwords.clone(),
            cc.egui_ctx.clone(),
        );

        let tray = match tray::create(&cc.egui_ctx) {
            Ok(t) => Some(t),
            Err(e) => {
                log::warn!("Icône de barre des menus indisponible : {e}");
                None
            }
        };

        let mut toast = None;
        let hotkey = match hotkey::register(&cc.egui_ctx) {
            Ok(m) => Some(m),
            Err(e) => {
                log::warn!("Raccourci ⌘⇧Z indisponible : {e}");
                toast = Some(Toast {
                    message: "⌘⇧Z déjà utilisé — CopyQ est-il encore lancé ?".into(),
                    at: Instant::now(),
                    duration: Duration::from_secs(8),
                    error: true,
                });
                None
            }
        };

        let mut app = Self {
            history,
            paused,
            clipboard: arboard::Clipboard::new().ok(),
            query: String::new(),
            selected: 0,
            scroll_to_selected: false,
            settings,
            save_passwords,
            detect_passwords,
            login_status: login_item::status(),
            confirm_clear: false,
            open_settings_on_start: std::env::var_os("COPYQ_OPEN_SETTINGS").is_some(),
            toast,
            cache: None,
            _tray: tray,
            _hotkey: hotkey,
        };
        // Mots de passe copiés avant la détection (ex. depuis Dashlane) : rattrapage.
        if app.settings.detect_passwords {
            app.reclassify();
        }
        app
    }

    fn rows(&mut self) -> Arc<Vec<Row>> {
        let h = self.history.lock_safe();
        if let Some((version, query, rows)) = &self.cache {
            if *version == h.version() && *query == self.query {
                return rows.clone();
            }
        }
        let rows: Arc<Vec<Row>> = Arc::new(
            h.search(&self.query)
                .into_iter()
                .map(|i| {
                    let it = &h.items()[i];
                    Row {
                        index: i,
                        title: if it.sensitive {
                            "••••••••••••".to_string()
                        } else {
                            history::preview(&it.text, PREVIEW_CHARS)
                        },
                        kind: if it.sensitive { Kind::Secret } else { Kind::detect(&it.text) },
                        sensitive: it.sensitive,
                        copied_at: it.copied_at,
                        lines: it.text.trim().lines().count().max(1),
                        chars: it.text.chars().count(),
                    }
                })
                .collect(),
        );
        self.cache = Some((h.version(), self.query.clone(), rows.clone()));
        rows
    }

    /// Remet l'élément dans le presse-papiers et le remonte en tête.
    fn copy_item(&mut self, ctx: &egui::Context, index: usize) {
        let text = {
            let h = self.history.lock_safe();
            match h.items().get(index) {
                Some(it) => it.text.clone(),
                None => return,
            }
        };
        let Some(cb) = self.clipboard.as_mut() else {
            self.notify_error("Presse-papiers inaccessible");
            return;
        };
        if let Err(e) = cb.set_text(text.clone()) {
            self.notify_error(format!("Échec de la copie : {e}"));
            return;
        }
        self.history.lock_safe().push(text);
        self.selected = 0;
        self.scroll_to_selected = true;
        self.notify("Copié dans le presse-papiers");
        if self.settings.hide_after_copy {
            self.query.clear();
            platform::hide_window(ctx);
        }
    }

    fn delete_item(&mut self, index: usize) {
        self.history.lock_safe().remove(index);
    }

    fn clear_all(&mut self) {
        self.history.lock_safe().clear();
        self.selected = 0;
    }

    /// Contenu du panneau « Réglages ».
    fn settings_ui(&mut self, ui: &mut Ui, p: &Palette) {
        ui.set_width(310.0);
        ui.spacing_mut().item_spacing.y = 4.0;
        ui.label(RichText::new("Réglages").size(13.5).color(p.text).strong());
        ui.add_space(6.0);

        let mut changed = false;
        ui.horizontal(|ui| {
            changed |= toggle(ui, &mut self.settings.hide_after_copy, "Masquer après copie", p)
                .changed();
        });
        caption(ui, "La fenêtre se ferme dès qu'un élément est choisi : il reste à faire ⌘V.", p);
        ui.add_space(8.0);

        let available = self.login_status != login_item::Status::Unavailable;
        let mut on = matches!(
            self.login_status,
            login_item::Status::Enabled | login_item::Status::RequiresApproval
        );
        let resp = ui
            .add_enabled_ui(available, |ui| {
                ui.horizontal(|ui| toggle(ui, &mut on, "Ouvrir au démarrage", p)).inner
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
            if toggle(ui, &mut self.settings.save_passwords, "Enregistrer les mots de passe", p)
                .changed()
            {
                changed = true;
                self.save_passwords
                    .store(self.settings.save_passwords, Ordering::Relaxed);
                log::info!("enregistrement des mots de passe : {}", self.settings.save_passwords);
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
            if toggle(ui, &mut self.settings.detect_passwords, "Détecter les mots de passe", p)
                .changed()
            {
                changed = true;
                self.detect_passwords
                    .store(self.settings.detect_passwords, Ordering::Relaxed);
                log::info!("détection des mots de passe : {}", self.settings.detect_passwords);
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

        if changed {
            self.settings.save(&settings::default_path());
        }
    }

    fn toggle_sensitive(&mut self, index: usize) {
        let now_sensitive = {
            let mut h = self.history.lock_safe();
            let Some(current) = h.items().get(index).map(|it| it.sensitive) else { return };
            h.set_sensitive(index, !current);
            !current
        };
        if now_sensitive {
            self.notify("Marqué comme mot de passe : chiffré et masqué");
        } else {
            self.notify("Affiché en clair");
        }
    }

    /// Applique la détection aux éléments déjà enregistrés.
    fn reclassify(&mut self) {
        let n = self
            .history
            .lock_safe()
            .reclassify_probable_passwords(self.settings.save_passwords);
        if n > 0 {
            let what = if self.settings.save_passwords { "chiffré(s) et masqué(s)" } else { "supprimé(s)" };
            log::info!("{n} mot(s) de passe probable(s) déjà enregistré(s) : {what}");
            self.notify(format!("{n} mot(s) de passe probable(s) {what}"));
        }
    }

    fn set_login_item(&mut self, enabled: bool) {
        match login_item::set_enabled(enabled) {
            Ok(()) => {
                self.login_status = login_item::status();
                match self.login_status {
                    login_item::Status::RequiresApproval => self.notify_error(
                        "À valider dans Réglages → Général → Ouverture",
                    ),
                    login_item::Status::Enabled => self.notify("Ouverture au démarrage activée"),
                    _ => self.notify("Ouverture au démarrage désactivée"),
                }
            }
            Err(e) => {
                log::warn!("lancement au démarrage : {e}");
                self.notify_error(format!("Impossible : {e}"));
            }
        }
    }

    fn notify(&mut self, msg: impl Into<String>) {
        self.toast = Some(Toast {
            message: msg.into(),
            at: Instant::now(),
            duration: Duration::from_millis(1800),
            error: false,
        });
    }

    fn notify_error(&mut self, msg: impl Into<String>) {
        self.toast = Some(Toast {
            message: msg.into(),
            at: Instant::now(),
            duration: Duration::from_secs(5),
            error: true,
        });
    }
}

impl eframe::App for CopyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Captures d'écran CI : ouvrir le panneau des réglages au démarrage.
        if std::mem::take(&mut self.open_settings_on_start) {
            ctx.memory_mut(|m| m.open_popup(egui::Id::new(SETTINGS_POPUP)));
        }
        // Fermer la fenêtre = la masquer ; l'application reste dans la barre des menus.
        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            platform::hide_window(ctx);
        }

        let p = theme::palette_ctx(ctx);
        let rows = self.rows();
        let total = self.history.lock_safe().len();
        if self.selected >= rows.len() {
            self.selected = rows.len().saturating_sub(1);
        }

        // ── Navigation clavier ─────────────────────────────────────────────
        let (up, down, enter, esc, delete, mark) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::ArrowUp),
                i.key_pressed(egui::Key::ArrowDown),
                i.key_pressed(egui::Key::Enter),
                i.key_pressed(egui::Key::Escape),
                i.modifiers.command && i.key_pressed(egui::Key::Backspace),
                i.modifiers.command && i.key_pressed(egui::Key::L),
            )
        });
        if down && self.selected + 1 < rows.len() {
            self.selected += 1;
            self.scroll_to_selected = true;
        }
        if up && self.selected > 0 {
            self.selected -= 1;
            self.scroll_to_selected = true;
        }
        let mut to_copy: Option<usize> = None;
        let mut to_delete: Option<usize> = None;
        if enter {
            to_copy = rows.get(self.selected).map(|r| r.index);
        }
        if delete {
            to_delete = rows.get(self.selected).map(|r| r.index);
        }
        let mut to_toggle: Option<usize> = None;
        if mark {
            to_toggle = rows.get(self.selected).map(|r| r.index);
        }
        if esc {
            if self.query.is_empty() {
                platform::hide_window(ctx);
            } else {
                self.query.clear();
            }
        }

        // ── En-tête : zone déplaçable + barre de recherche ─────────────────
        egui::TopBottomPanel::top("header")
            .show_separator_line(false)
            .frame(Frame::none().fill(p.bg).inner_margin(Margin {
                left: 14.0,
                right: 14.0,
                top: 0.0,
                bottom: 10.0,
            }))
            .show(ctx, |ui| {
                let (strip, drag) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), TITLEBAR_HEIGHT),
                    Sense::click_and_drag(),
                );
                if drag.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                ui.painter().text(
                    strip.center(),
                    Align2::CENTER_CENTER,
                    "Presse-papiers",
                    FontId::proportional(12.5),
                    p.text_faint,
                );
                ui.add_space(4.0);

                Frame::none()
                    .fill(p.surface)
                    .stroke(Stroke::new(1.0_f32, p.border))
                    .rounding(10.0)
                    .inner_margin(Margin::symmetric(12.0, 9.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("🔍").size(15.0).color(p.text_faint));
                            let count = if self.query.is_empty() {
                                format!("{total}")
                            } else {
                                format!("{} / {total}", rows.len())
                            };
                            let edit = ui.add(
                                egui::TextEdit::singleline(&mut self.query)
                                    .frame(false)
                                    .font(FontId::proportional(16.0))
                                    .text_color(p.text)
                                    .hint_text(
                                        RichText::new("Rechercher dans l'historique…")
                                            .color(p.text_faint),
                                    )
                                    .desired_width(ui.available_width() - 64.0),
                            );
                            if edit.changed() {
                                self.selected = 0;
                                self.scroll_to_selected = true;
                            }
                            // Garde le focus dans la recherche pour taper directement.
                            if ctx.memory(|m| m.focused().is_none()) {
                                edit.request_focus();
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(RichText::new(count).size(12.0).color(p.text_faint));
                                },
                            );
                        });
                    });
            });

        // ── Pied de page : réglages + raccourcis ───────────────────────────
        egui::TopBottomPanel::bottom("footer")
            .show_separator_line(false)
            .frame(Frame::none().fill(p.bg).inner_margin(Margin {
                left: 14.0,
                right: 14.0,
                top: 10.0,
                bottom: 10.0,
            }))
            .show(ctx, |ui| {
                let r = ui.clip_rect();
                ui.painter()
                    .hline(r.x_range(), r.top(), Stroke::new(1.0_f32, p.border));

                ui.horizontal(|ui| {
                    let mut paused = self.paused.load(Ordering::Relaxed);
                    if toggle(ui, &mut paused, "Pause", &p)
                        .on_hover_text("Ne plus enregistrer les nouvelles copies (mots de passe…)")
                        .changed()
                    {
                        self.paused.store(paused, Ordering::Relaxed);
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.confirm_clear {
                            if ghost_button(ui, "Annuler", p.text_muted, &p).clicked() {
                                self.confirm_clear = false;
                            }
                            let confirm = egui::Button::new(
                                RichText::new("Tout supprimer").color(Color32::WHITE),
                            )
                            .fill(p.danger)
                            .rounding(7.0);
                            if ui.add(confirm).clicked() {
                                self.clear_all();
                                self.confirm_clear = false;
                            }
                        } else if total > 0
                            && ghost_button(ui, "Tout effacer", p.danger, &p).clicked()
                        {
                            self.confirm_clear = true;
                        }
                        let button = ghost_button(ui, "Réglages", p.text_muted, &p);
                        let popup_id = egui::Id::new(SETTINGS_POPUP);
                        if button.clicked() {
                            ui.memory_mut(|m| m.toggle_popup(popup_id));
                        }
                        egui::popup::popup_above_or_below_widget(
                            ui,
                            popup_id,
                            &button,
                            egui::AboveOrBelow::Above,
                            egui::popup::PopupCloseBehavior::CloseOnClickOutside,
                            |ui| self.settings_ui(ui, &p),
                        );
                    });
                });
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 5.0;
                    for (key, label) in [
                        ("⌘⇧Z", "ouvrir"),
                        ("↑↓", "naviguer"),
                        ("entrée", "copier"),
                        ("⌘⌫", "supprimer"),
                        ("esc", "fermer"),
                    ] {
                        kbd(ui, key, &p);
                        ui.label(RichText::new(label).size(11.5).color(p.text_faint));
                        ui.add_space(6.0);
                    }
                });
            });

        // ── Liste ──────────────────────────────────────────────────────────
        egui::CentralPanel::default()
            .frame(Frame::none().fill(p.bg).inner_margin(Margin::symmetric(8.0, 0.0)))
            .show(ctx, |ui| {
                if rows.is_empty() {
                    empty_state(ui, total == 0, &self.query, &p);
                    return;
                }

                let now = history::now_secs();
                let hist = self.history.clone();
                let scroll_to_selected = self.scroll_to_selected;
                let selected = self.selected;
                let mut clicked: Option<(usize, usize)> = None;
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        for (pos, row) in rows.iter().enumerate() {
                            let action = row_ui(
                                ui,
                                row,
                                pos == selected,
                                scroll_to_selected,
                                now,
                                &hist,
                                &p,
                            );
                            match action {
                                RowAction::Copy => clicked = Some((pos, row.index)),
                                RowAction::Delete => to_delete = Some(row.index),
                                RowAction::ToggleSensitive => to_toggle = Some(row.index),
                                RowAction::None => {}
                            }
                        }
                        ui.add_space(6.0);
                    });
                self.scroll_to_selected = false;
                if let Some((pos, index)) = clicked {
                    self.selected = pos;
                    to_copy = Some(index);
                }
            });

        self.show_toast(ctx, &p);

        if let Some(i) = to_delete {
            self.delete_item(i);
        } else if let Some(i) = to_toggle {
            self.toggle_sensitive(i);
        } else if let Some(i) = to_copy {
            self.copy_item(ctx, i);
        }
    }
}

impl CopyApp {
    fn show_toast(&mut self, ctx: &egui::Context, p: &Palette) {
        let Some(toast) = &self.toast else { return };
        let elapsed = toast.at.elapsed();
        if elapsed >= toast.duration {
            self.toast = None;
            return;
        }
        // Fondu sur les 300 dernières ms.
        let remaining = (toast.duration - elapsed).as_secs_f32();
        let alpha = (remaining / 0.3).min(1.0);
        let dot = if toast.error { p.danger } else { p.success };
        egui::Area::new(egui::Id::new("toast"))
            .anchor(Align2::CENTER_BOTTOM, egui::vec2(0.0, -86.0))
            .interactable(false)
            .show(ctx, |ui| {
                Frame::none()
                    .fill(p.text.gamma_multiply(alpha))
                    .rounding(18.0)
                    .inner_margin(Margin::symmetric(14.0, 7.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("●").size(10.0).color(dot.gamma_multiply(alpha)));
                            ui.label(
                                RichText::new(toast.message.as_str())
                                    .size(13.0)
                                    .color(p.bg.gamma_multiply(alpha)),
                            );
                        });
                    });
            });
        ctx.request_repaint_after(Duration::from_millis(30));
    }
}

enum RowAction {
    None,
    Copy,
    Delete,
    ToggleSensitive,
}

/// Dessine une ligne de l'historique façon « carte » : icône, aperçu, métadonnées.
fn row_ui(
    ui: &mut Ui,
    row: &Row,
    selected: bool,
    scroll_to_selected: bool,
    now: u64,
    hist: &Mutex<History>,
    p: &Palette,
) -> RowAction {
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW_HEIGHT), Sense::click());
    if selected && scroll_to_selected {
        resp.scroll_to_me(None);
    }
    if !ui.is_rect_visible(rect) {
        return RowAction::None;
    }
    let hovered = ui.rect_contains_pointer(rect);
    let painter = ui.painter_at(rect);

    // Fond.
    if selected {
        painter.rect_filled(rect, 10.0, p.accent_soft);
        painter.rect_filled(
            Rect::from_min_size(
                egui::pos2(rect.left() + 1.0, rect.top() + 15.0),
                egui::vec2(3.0, rect.height() - 30.0),
            ),
            2.0,
            p.accent,
        );
    } else if hovered {
        painter.rect_filled(rect, 10.0, p.surface_hover);
    }

    // Icône du type de contenu.
    let inner = rect.shrink2(egui::vec2(12.0, 0.0));
    let tile = Rect::from_center_size(
        egui::pos2(inner.left() + 15.0, rect.center().y),
        egui::vec2(30.0, 30.0),
    );
    painter.rect_filled(tile, 8.0, if selected { p.surface } else { p.surface_hover });
    painter.rect_stroke(tile, 8.0, Stroke::new(1.0_f32, p.border));
    row.kind
        .paint(&painter, tile.center(), if selected { p.accent } else { p.text_muted });

    // Texte.
    let text_x = tile.right() + 12.0;
    let max_w = (inner.right() - 72.0 - text_x).max(40.0);
    let title = single_line(ui, &row.title, 14.0, p.text, max_w);
    let meta = single_line(ui, &meta_text(row, now), 11.5, p.text_faint, max_w);
    let total_h = title.size().y + 3.0 + meta.size().y;
    let top = rect.center().y - total_h / 2.0;
    painter.galley(egui::pos2(text_x, top), title.clone(), p.text);
    painter.galley(
        egui::pos2(text_x, top + title.size().y + 3.0),
        meta,
        p.text_faint,
    );

    // Côté droit : bouton supprimer au survol, sinon indice « ↵ » sur la sélection.
    let action_rect = Rect::from_center_size(
        egui::pos2(inner.right() - 12.0, rect.center().y),
        egui::vec2(26.0, 26.0),
    );
    let mut action = RowAction::None;
    if hovered {
        let del = ui
            .interact(action_rect, resp.id.with("delete"), Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text("Supprimer");
        if del.hovered() {
            painter.rect_filled(action_rect, 7.0, p.danger.gamma_multiply(0.15));
        }
        icons::trash(
            &painter,
            action_rect.center(),
            if del.hovered() { p.danger } else { p.text_faint },
        );
        if del.clicked() {
            action = RowAction::Delete;
        }

        let lock_rect = action_rect.translate(egui::vec2(-30.0, 0.0));
        let lock = ui
            .interact(lock_rect, resp.id.with("lock"), Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(if row.sensitive {
                "Ce n'est pas un mot de passe : afficher en clair (⌘L)"
            } else {
                "C'est un mot de passe : chiffrer et masquer (⌘L)"
            });
        if lock.hovered() {
            painter.rect_filled(lock_rect, 7.0, p.accent.gamma_multiply(0.15));
        }
        icons::lock(
            &painter,
            lock_rect.center(),
            if lock.hovered() || row.sensitive { p.accent } else { p.text_faint },
        );
        if lock.clicked() {
            action = RowAction::ToggleSensitive;
        }
    } else if selected {
        icons::enter(&painter, action_rect.center(), p.accent);
    }

    if matches!(action, RowAction::None) && resp.clicked() {
        action = RowAction::Copy;
    }
    resp.on_hover_ui(|ui| {
        ui.set_max_width(440.0);
        let text = if row.sensitive {
            "Mot de passe masqué — cliquez ou ↵ pour le copier".to_string()
        } else {
            tooltip(hist, row.index)
        };
        ui.label(RichText::new(text).size(12.5));
    });
    action
}

fn meta_text(row: &Row, now: u64) -> String {
    if row.sensitive {
        return format!("mot de passe  ·  {}", history::relative_time(row.copied_at, now));
    }
    let mut parts = vec![history::relative_time(row.copied_at, now)];
    if row.lines > 1 {
        parts.push(format!("{} lignes", row.lines));
    }
    parts.push(if row.chars < 1000 {
        format!("{} car.", row.chars)
    } else {
        format!("{:.1} k car.", row.chars as f32 / 1000.0).replace('.', ",")
    });
    parts.join("  ·  ")
}

fn single_line(
    ui: &Ui,
    text: &str,
    size: f32,
    color: Color32,
    max_width: f32,
) -> Arc<egui::Galley> {
    let mut job = LayoutJob::single_section(
        text.to_owned(),
        egui::TextFormat::simple(FontId::proportional(size), color),
    );
    job.wrap = TextWrapping::truncate_at_width(max_width);
    ui.fonts(|f| f.layout_job(job))
}

/// Texte complet (tronqué) affiché au survol d'un élément.
fn tooltip(history: &Mutex<History>, index: usize) -> String {
    let h = history.lock_safe();
    h.items()
        .get(index)
        .map(|it| {
            let mut s: String = it.text.chars().take(TOOLTIP_CHARS).collect();
            if it.text.chars().count() > TOOLTIP_CHARS {
                s.push_str("\n…");
            }
            s
        })
        .unwrap_or_default()
}

fn empty_state(ui: &mut Ui, history_empty: bool, query: &str, p: &Palette) {
    ui.vertical_centered(|ui| {
        ui.add_space((ui.available_height() / 2.0 - 60.0).max(20.0));
        let (icon, title, subtitle) = if history_empty {
            (
                "📋",
                "Aucun élément pour l'instant".to_string(),
                "Copiez du texte avec ⌘C, il apparaîtra ici.".to_string(),
            )
        } else {
            (
                "🔍",
                "Aucun résultat".to_string(),
                format!("Rien ne correspond à « {query} »."),
            )
        };
        ui.label(RichText::new(icon).size(34.0).color(p.text_faint));
        ui.add_space(8.0);
        ui.label(RichText::new(title).size(15.0).color(p.text));
        ui.add_space(2.0);
        ui.label(RichText::new(subtitle).size(12.5).color(p.text_muted));
    });
}

/// Interrupteur façon iOS / macOS, avec libellé cliquable.
fn toggle(ui: &mut Ui, on: &mut bool, label: &str, p: &Palette) -> egui::Response {
    let size = egui::vec2(30.0, 18.0);
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
    let label_resp = ui.add(
        egui::Label::new(RichText::new(label).size(12.5).color(p.text_muted)).sense(Sense::click()),
    );
    if resp.clicked() || label_resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool(resp.id, *on);
    let off_color = if ui.visuals().dark_mode {
        Color32::from_rgb(0x48, 0x48, 0x52)
    } else {
        Color32::from_rgb(0xD6, 0xD6, 0xDE)
    };
    let track = lerp_color(off_color, p.accent, t);
    let radius = rect.height() / 2.0;
    ui.painter().rect_filled(rect, Rounding::same(radius), track);
    let x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), t);
    ui.painter()
        .circle_filled(egui::pos2(x, rect.center().y), radius - 2.5, Color32::WHITE);
    resp | label_resp
}

fn caption(ui: &mut Ui, text: &str, p: &Palette) {
    ui.add(egui::Label::new(RichText::new(text).size(11.5).color(p.text_faint)).wrap());
}

fn ghost_button(ui: &mut Ui, text: &str, color: Color32, p: &Palette) -> egui::Response {
    let _ = p;
    ui.add(
        egui::Button::new(RichText::new(text).size(12.5).color(color))
            .fill(Color32::TRANSPARENT)
            .rounding(7.0),
    )
}

/// Petite touche de clavier stylisée.
fn kbd(ui: &mut Ui, key: &str, p: &Palette) {
    Frame::none()
        .fill(p.surface)
        .stroke(Stroke::new(1.0_f32, p.border))
        .rounding(4.0)
        .inner_margin(Margin::symmetric(5.0, 1.0))
        .show(ui, |ui| {
            ui.label(RichText::new(key).size(11.0).color(p.text_muted));
        });
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}
