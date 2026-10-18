//! Interface egui : recherche, liste de l'historique, navigation clavier.
//!
//! - `row` : calcul et dessin des lignes de la liste ;
//! - `settings_panel` : panneau « Réglages » (onglets Général et Limites) ;
//! - `widgets` : petits composants réutilisables.

mod row;
mod settings_panel;
mod widgets;

use crate::history::{self, History};
use crate::logging::LockExt;
use crate::settings::{self, Settings};
use crate::theme::{self, Palette};
use crate::{clipboard, hotkey, i18n, login_item, paste, platform, tray};
use eframe::egui::{self, Align2, Color32, FontId, Frame, Margin, RichText, Sense, Stroke, Ui};
use row::{build_row, image_hashes, load_thumbnail, row_ui, Row, RowAction};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use widgets::{empty_state, ghost_button, kbd, toggle};

const SETTINGS_POPUP: &str = "settings_popup";
/// Hauteur réservée en haut pour les boutons de fenêtre macOS (barre de titre transparente).
const TITLEBAR_HEIGHT: f32 = 28.0;

struct Toast {
    message: String,
    at: Instant,
    duration: Duration,
    error: bool,
}

pub struct CopyApp {
    history: Arc<Mutex<History>>,
    /// Réglages et états partagés avec la surveillance du presse-papiers.
    shared: clipboard::Shared,
    clipboard: Option<arboard::Clipboard>,
    query: String,
    selected: usize,
    scroll_to_selected: bool,
    settings: Settings,
    /// Vignettes chargées (empreinte → texture), `None` si illisible.
    thumbs: HashMap<String, Option<egui::TextureHandle>>,
    confirm_clear: bool,
    open_settings_on_start: bool,
    settings_tab: SettingsTab,
    /// Valeurs en cours de modification dans l'onglet « Limites ».
    limits_draft: Option<Settings>,
    last_expire: Instant,
    login_status: login_item::Status,
    toast: Option<Toast>,
    /// (version de l'historique, requête) → lignes.
    cache: Option<(u64, String, Arc<Vec<Row>>)>,
    /// Lignes de tous les éléments (par version de l'historique) : la recherche ne
    /// fait plus que filtrer, sans recalculer aperçus ni accès disque à chaque frappe.
    all_rows: Option<(u64, Arc<Vec<Row>>)>,
    tray: Option<tray::Tray>,
    _hotkey: Option<global_hotkey::GlobalHotKeyManager>,
}

impl CopyApp {
    pub fn new(cc: &eframe::CreationContext<'_>, history: Arc<Mutex<History>>) -> Self {
        paste::init();

        let settings = Settings::load(&settings::default_path());
        log::info!("réglages : {settings:?}");
        // Langue : réglage (« auto » = celle de macOS) ; COPYQ_LANG la force (captures CI).
        let lang = std::env::var("COPYQ_LANG")
            .ok()
            .and_then(|code| i18n::Lang::from_code(&code))
            .unwrap_or_else(|| i18n::resolve(&settings.language));
        i18n::set(lang);
        log::info!("langue de l'interface : {}", lang.code());
        theme::install(&cc.egui_ctx, lang);
        let shared = clipboard::Shared::new(
            settings.save_passwords,
            settings.detect_passwords,
            settings.save_images,
        );
        clipboard::spawn_watcher(history.clone(), shared.clone(), cc.egui_ctx.clone());

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
                    message: t!("toast.hotkey_taken").into(),
                    at: Instant::now(),
                    duration: Duration::from_secs(8),
                    error: true,
                });
                None
            }
        };

        let mut app = Self {
            history,
            shared,
            clipboard: arboard::Clipboard::new().ok(),
            query: String::new(),
            selected: 0,
            scroll_to_selected: false,
            settings,
            thumbs: HashMap::new(),
            login_status: login_item::status(),
            confirm_clear: false,
            open_settings_on_start: std::env::var_os("COPYQ_OPEN_SETTINGS").is_some(),
            settings_tab: if std::env::var("COPYQ_OPEN_SETTINGS").as_deref() == Ok("limits") {
                SettingsTab::Limits
            } else {
                SettingsTab::General
            },
            limits_draft: None,
            last_expire: Instant::now(),
            toast,
            cache: None,
            all_rows: None,
            tray,
            _hotkey: hotkey,
        };
        // Limites choisies (dont durée de conservation).
        app.apply_limits();
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
        if self.all_rows.as_ref().map(|(v, _)| *v) != Some(h.version()) {
            let all: Vec<Row> = h
                .items()
                .iter()
                .enumerate()
                .map(|(i, it)| build_row(i, it))
                .collect();
            self.all_rows = Some((h.version(), Arc::new(all)));
            // Vignette absente : l'image était peut-être encore en cours d'écriture.
            self.thumbs.retain(|_, t| t.is_some());
        }
        let all = self
            .all_rows
            .as_ref()
            .map(|(_, r)| r.clone())
            .unwrap_or_default();
        let rows: Arc<Vec<Row>> = if self.query.trim().is_empty() {
            all
        } else {
            Arc::new(
                h.search(&self.query)
                    .into_iter()
                    .filter_map(|i| all.get(i).cloned())
                    .collect(),
            )
        };
        self.cache = Some((h.version(), self.query.clone(), rows.clone()));
        rows
    }

    /// Remet l'élément dans le presse-papiers et le remonte en tête.
    fn copy_item(&mut self, ctx: &egui::Context, index: usize) {
        let item = {
            let h = self.history.lock_safe();
            match h.items().get(index) {
                Some(it) => it.clone(),
                None => return,
            }
        };
        let Some(cb) = self.clipboard.as_mut() else {
            self.notify_error(t!("toast.clipboard_unavailable"));
            return;
        };
        if let Err(e) = clipboard::write_item(cb, &item) {
            self.notify_error(t!("toast.copy_failed", error = e));
            return;
        }
        // Cette écriture vient de nous : la surveillance ne doit pas la réenregistrer.
        self.shared.mark_own_write();
        self.history.lock_safe().push_item(item);
        self.selected = 0;
        self.scroll_to_selected = true;
        if self.settings.auto_paste {
            // Masquer d'abord : l'application d'origine reprend la main, puis ⌘V.
            self.query.clear();
            platform::hide_window(ctx);
            match paste::paste_into_previous() {
                paste::Outcome::Pasted => {}
                paste::Outcome::NotTrusted => {
                    self.notify_error(t!("toast.not_trusted"));
                }
                paste::Outcome::NoTarget => self.notify(t!("toast.copied_press_v")),
            }
            return;
        }
        self.notify(t!("toast.copied"));
        if self.settings.hide_after_copy {
            self.query.clear();
            platform::hide_window(ctx);
        }
    }

    fn delete_item(&mut self, index: usize) {
        self.history.lock_safe().remove(index);
        self.prune_thumbs();
    }

    fn clear_all(&mut self) {
        self.history.lock_safe().clear();
        self.selected = 0;
        self.prune_thumbs();
    }

    /// Libère les textures des vignettes dont l'image n'est plus dans l'historique.
    fn prune_thumbs(&mut self) {
        if self.thumbs.is_empty() {
            return;
        }
        let history = self.history.lock_safe();
        let live = image_hashes(&history);
        let before = self.thumbs.len();
        self.thumbs.retain(|hash, _| live.contains(hash.as_str()));
        drop(history);
        if self.thumbs.len() < before {
            log::debug!("{} vignette(s) libérée(s)", before - self.thumbs.len());
        }
    }

    /// Applique les limites des réglages à l'historique et à la surveillance.
    fn apply_limits(&mut self) -> usize {
        self.shared
            .max_image_pixels
            .store(self.settings.max_image_pixels(), Ordering::Relaxed);
        self.shared
            .max_images
            .store(self.settings.max_images, Ordering::Relaxed);
        let removed = self
            .history
            .lock_safe()
            .set_limits(self.settings.limits(), history::now_secs());
        if removed > 0 {
            log::info!("limites appliquées : {removed} élément(s) supprimé(s)");
            self.prune_thumbs();
        }
        removed
    }

    fn toggle_sensitive(&mut self, index: usize) {
        let now_sensitive = {
            let mut h = self.history.lock_safe();
            let Some(current) = h.items().get(index).map(|it| it.sensitive) else {
                return;
            };
            h.set_sensitive(index, !current);
            !current
        };
        if now_sensitive {
            self.notify(t!("toast.marked_password"));
        } else {
            self.notify(t!("toast.shown_plain"));
        }
    }

    /// Applique la détection aux éléments déjà enregistrés.
    fn reclassify(&mut self) {
        let n = self
            .history
            .lock_safe()
            .reclassify_probable_passwords(self.settings.save_passwords);
        if n > 0 {
            let what = if self.settings.save_passwords {
                "chiffré(s) et masqué(s)"
            } else {
                "supprimé(s)"
            };
            log::info!("{n} mot(s) de passe probable(s) déjà enregistré(s) : {what}");
            self.notify(if self.settings.save_passwords {
                t!("toast.probable_encrypted", n = n)
            } else {
                t!("toast.probable_removed", n = n)
            });
        }
    }

    fn set_login_item(&mut self, enabled: bool) {
        match login_item::set_enabled(enabled) {
            Ok(()) => {
                self.login_status = login_item::status();
                match self.login_status {
                    login_item::Status::RequiresApproval => {
                        self.notify_error(t!("toast.login_needs_approval"))
                    }
                    login_item::Status::Enabled => self.notify(t!("toast.login_on")),
                    _ => self.notify(t!("toast.login_off")),
                }
            }
            Err(e) => {
                log::warn!("lancement au démarrage : {e}");
                self.notify_error(t!("toast.login_failed", error = e));
            }
        }
    }

    /// Change la langue de l'interface (réglage) : textes, polices, menu de l'icône.
    fn apply_language(&mut self, ctx: &egui::Context) {
        let lang = i18n::resolve(&self.settings.language);
        i18n::set(lang);
        log::info!("langue de l'interface : {}", lang.code());
        theme::install_fonts(ctx, lang);
        if let Some(tray) = &self.tray {
            tray.relabel();
        }
        // Lignes en cache : libellés (« Image », « 2 fichiers »…) à recalculer.
        self.all_rows = None;
        self.cache = None;
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

/// Actions demandées pendant l'image (clavier ou souris), exécutées à la fin,
/// une fois l'affichage terminé. Priorité : supprimer, puis marquer, puis copier.
#[derive(Default)]
struct Pending {
    copy: Option<usize>,
    delete: Option<usize>,
    toggle: Option<usize>,
}

impl eframe::App for CopyApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = &ui.ctx().clone();
        self.housekeeping(ctx);

        let p = theme::palette_ctx(ctx);
        let rows = self.rows();
        let total = self.history.lock_safe().len();
        if self.selected >= rows.len() {
            self.selected = rows.len().saturating_sub(1);
        }

        let mut pending = self.handle_keys(ctx, &rows);
        self.header_ui(ui, ctx, &p, rows.len(), total);
        self.footer_ui(ui, &p, total);
        self.list_ui(ui, ctx, &p, &rows, total, &mut pending);
        self.show_toast(ctx, &p);

        if let Some(i) = pending.delete {
            self.delete_item(i);
        } else if let Some(i) = pending.toggle {
            self.toggle_sensitive(i);
        } else if let Some(i) = pending.copy {
            self.copy_item(ctx, i);
        }
    }
}

impl CopyApp {
    /// Tâches de fond et événements de fenêtre, en début d'image.
    fn housekeeping(&mut self, ctx: &egui::Context) {
        // Toutes les minutes : durée de conservation, puis vignettes devenues inutiles
        // (images évincées par de nouvelles copies dans le fil de surveillance).
        if self.last_expire.elapsed() > Duration::from_secs(60) {
            self.last_expire = Instant::now();
            if self.settings.retention_days > 0 {
                let n = self.history.lock_safe().expire(history::now_secs());
                if n > 0 {
                    log::info!("{n} élément(s) expiré(s)");
                }
            }
            self.prune_thumbs();
        }
        // Captures d'écran CI : ouvrir le panneau des réglages au démarrage.
        if std::mem::take(&mut self.open_settings_on_start) {
            egui::Popup::open_id(ctx, egui::Id::unique(SETTINGS_POPUP));
        }
        // Fermer la fenêtre = la masquer ; l'application reste dans la barre des menus.
        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            platform::hide_window(ctx);
        }
    }

    /// Navigation clavier : ↑↓, entrée, ⌘⌫, ⌘L, échap.
    fn handle_keys(&mut self, ctx: &egui::Context, rows: &[Row]) -> Pending {
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
        if esc {
            if self.query.is_empty() {
                platform::hide_window(ctx);
            } else {
                self.query.clear();
            }
        }
        let current = rows.get(self.selected).map(|r| r.index);
        Pending {
            copy: current.filter(|_| enter),
            delete: current.filter(|_| delete),
            toggle: current.filter(|_| mark),
        }
    }

    /// En-tête : zone déplaçable + barre de recherche.
    fn header_ui(
        &mut self,
        root: &mut Ui,
        ctx: &egui::Context,
        p: &Palette,
        shown: usize,
        total: usize,
    ) {
        egui::Panel::top("header")
            .show_separator_line(false)
            .frame(Frame::NONE.fill(p.bg).inner_margin(Margin {
                left: 14,
                right: 14,
                top: 0,
                bottom: 10,
            }))
            .show(root, |ui| {
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
                    t!("app.title"),
                    FontId::proportional(12.5),
                    p.text_faint,
                );
                ui.add_space(4.0);

                Frame::NONE
                    .fill(p.surface)
                    .stroke(Stroke::new(1.0_f32, p.border))
                    .corner_radius(10.0)
                    .inner_margin(Margin::symmetric(12, 9))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| self.search_field(ctx, ui, p, shown, total));
                    });
            });
    }

    fn search_field(
        &mut self,
        ctx: &egui::Context,
        ui: &mut Ui,
        p: &Palette,
        shown: usize,
        total: usize,
    ) {
        ui.label(RichText::new("🔍").size(15.0).color(p.text_faint));
        let count = if self.query.is_empty() {
            format!("{total}")
        } else {
            format!("{shown} / {total}")
        };
        let edit = ui.add(
            egui::TextEdit::singleline(&mut self.query)
                .frame(Frame::NONE)
                // Même marge intérieure qu'avant egui 0.37 : texte centré sur la loupe.
                .margin(Margin::symmetric(4, 2))
                .font(FontId::proportional(16.0))
                .text_color(p.text)
                .hint_text(RichText::new(t!("search.hint")).color(p.text_faint))
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
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(count).size(12.0).color(p.text_faint));
        });
    }

    /// Pied de page : pause, réglages, effacement, rappel des raccourcis.
    fn footer_ui(&mut self, root: &mut Ui, p: &Palette, total: usize) {
        egui::Panel::bottom("footer")
            .show_separator_line(false)
            .frame(Frame::NONE.fill(p.bg).inner_margin(Margin {
                left: 14,
                right: 14,
                top: 10,
                bottom: 10,
            }))
            .show(root, |ui| {
                let r = ui.clip_rect();
                ui.painter()
                    .hline(r.x_range(), r.top(), Stroke::new(1.0_f32, p.border));
                ui.horizontal(|ui| {
                    let mut paused = self.shared.paused.load(Ordering::Relaxed);
                    if toggle(ui, &mut paused, t!("footer.pause"), p)
                        .on_hover_text(t!("footer.pause_tip"))
                        .changed()
                    {
                        self.shared.paused.store(paused, Ordering::Relaxed);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        self.footer_buttons(ui, p, total);
                    });
                });
                ui.add_space(6.0);
                self.shortcuts_ui(ui, p);
            });
    }

    /// « Tout effacer » (avec confirmation) et bouton du panneau « Réglages ».
    fn footer_buttons(&mut self, ui: &mut Ui, p: &Palette, total: usize) {
        if self.confirm_clear {
            if ghost_button(ui, t!("footer.cancel"), p.text_muted, p).clicked() {
                self.confirm_clear = false;
            }
            let confirm =
                egui::Button::new(RichText::new(t!("footer.delete_all")).color(Color32::WHITE))
                    .fill(p.danger)
                    .corner_radius(7.0);
            if ui.add(confirm).clicked() {
                self.clear_all();
                self.confirm_clear = false;
            }
        } else if total > 0 && ghost_button(ui, t!("footer.clear"), p.danger, p).clicked() {
            self.confirm_clear = true;
        }
        let button = ghost_button(ui, t!("footer.settings"), p.text_muted, p);
        egui::Popup::from_toggle_button_response(&button)
            .id(egui::Id::unique(SETTINGS_POPUP))
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .align(egui::RectAlign::TOP_END)
            .show(|ui| self.settings_ui(ui, p));
    }

    fn shortcuts_ui(&self, ui: &mut Ui, p: &Palette) {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            let enter = if self.settings.auto_paste {
                t!("keys.paste")
            } else {
                t!("keys.copy")
            };
            for (key, label) in [
                ("⌘⇧Z", t!("keys.open")),
                ("↑↓", t!("keys.navigate")),
                (t!("keys.enter"), enter),
                ("⌘⌫", t!("keys.delete")),
                ("esc", t!("keys.close")),
            ] {
                kbd(ui, key, p);
                ui.label(RichText::new(label).size(11.5).color(p.text_faint));
                ui.add_space(6.0);
            }
        });
    }

    /// Liste des éléments (ou message si elle est vide).
    fn list_ui(
        &mut self,
        root: &mut Ui,
        ctx: &egui::Context,
        p: &Palette,
        rows: &[Row],
        total: usize,
        pending: &mut Pending,
    ) {
        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(p.bg).inner_margin(Margin::symmetric(8, 0)))
            .show(root, |ui| {
                if rows.is_empty() {
                    empty_state(ui, total == 0, &self.query, p);
                    return;
                }
                // Vignettes : chargées une fois, puis gardées en cache.
                for row in rows {
                    if let Some(hash) = &row.thumb {
                        if !self.thumbs.contains_key(hash) {
                            if let Some(tex) = load_thumbnail(ctx, hash) {
                                self.thumbs.insert(hash.clone(), tex);
                            }
                        }
                    }
                }
                let now = history::now_secs();
                let mut clicked: Option<(usize, usize)> = None;
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        for (pos, row) in rows.iter().enumerate() {
                            let thumb = row
                                .thumb
                                .as_ref()
                                .and_then(|h| self.thumbs.get(h))
                                .and_then(|t| t.as_ref());
                            let action = row_ui(
                                ui,
                                row,
                                thumb,
                                pos == self.selected,
                                self.scroll_to_selected,
                                now,
                                &self.history,
                                p,
                            );
                            match action {
                                RowAction::Copy => clicked = Some((pos, row.index)),
                                RowAction::Delete => pending.delete = Some(row.index),
                                RowAction::ToggleSensitive => pending.toggle = Some(row.index),
                                RowAction::None => {}
                            }
                        }
                        ui.add_space(6.0);
                    });
                self.scroll_to_selected = false;
                if let Some((pos, index)) = clicked {
                    self.selected = pos;
                    pending.copy = Some(index);
                }
            });
    }

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
        egui::Area::new(egui::Id::unique("toast"))
            .anchor(Align2::CENTER_BOTTOM, egui::vec2(0.0, -86.0))
            .interactable(false)
            .show(ctx, |ui| {
                Frame::NONE
                    .fill(p.text.gamma_multiply(alpha))
                    .corner_radius(18.0)
                    .inner_margin(Margin::symmetric(14, 7))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("●")
                                    .size(10.0)
                                    .color(dot.gamma_multiply(alpha)),
                            );
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

#[derive(Clone, Copy, PartialEq)]
enum SettingsTab {
    General,
    Limits,
}
