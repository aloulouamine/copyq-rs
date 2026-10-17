//! Lignes de la liste : calcul (aperçu, icône, statistiques) et dessin.

use super::widgets::single_line;
use crate::history::{self, History, ItemKind};
use crate::logging::LockExt;
use crate::theme::Palette;
use crate::{icons, media};
use eframe::egui::{self, Align2, Color32, FontId, Rect, RichText, Sense, Stroke, Ui};
use std::collections::HashSet;
use std::sync::Mutex;
use unicode_normalization::UnicodeNormalization;

const PREVIEW_CHARS: usize = 160;
const TOOLTIP_CHARS: usize = 1_500;
const ROW_HEIGHT: f32 = 54.0;

/// Nature du contenu, pour choisir l'icône de la ligne.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Link,
    Path,
    Email,
    Multiline,
    Text,
    Secret,
    Image,
    File,
    Folder,
}

impl Kind {
    fn detect(text: &str, stats: history::TextStats) -> Self {
        let t = text.trim_start();
        let single_word = stats.single_word;
        if single_word && (t.starts_with("http://") || t.starts_with("https://")) {
            Kind::Link
        } else if single_word && (t.starts_with('/') || t.starts_with("~/")) {
            Kind::Path
        } else if single_word && t.contains('@') && t.contains('.') {
            Kind::Email
        } else if stats.lines > 1 {
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
            Kind::Image => icons::photo(painter, center, color),
            Kind::File => icons::document(painter, center, color),
            Kind::Folder => icons::folder(painter, center, color),
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
#[derive(Clone)]
pub(super) struct Row {
    pub(super) index: usize,
    title: String,
    kind: Kind,
    copied_at: u64,
    lines: usize,
    chars: usize,
    sensitive: bool,
    item_kind: ItemKind,
    /// Image : empreinte (vignette) ; fichiers : chemins pour l'infobulle.
    pub(super) thumb: Option<String>,
    /// Description propre aux images / fichiers.
    detail: String,
}

pub(super) enum RowAction {
    None,
    Copy,
    Delete,
    ToggleSensitive,
}

/// Dessine une ligne de l'historique façon « carte » : icône, aperçu, métadonnées.
#[allow(clippy::too_many_arguments)]
pub(super) fn row_ui(
    ui: &mut Ui,
    row: &Row,
    thumb: Option<&egui::TextureHandle>,
    selected: bool,
    scroll_to_selected: bool,
    now: u64,
    hist: &Mutex<History>,
    p: &Palette,
) -> RowAction {
    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), row_height(row)),
        Sense::click(),
    );
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
    let text_x = if let Some(tex) = thumb {
        // Vignette, proportions respectées, dans un cadre de 96 × 60.
        let frame = Rect::from_min_size(
            egui::pos2(inner.left(), rect.center().y - 30.0),
            egui::vec2(96.0, 60.0),
        );
        let [w, h] = tex.size();
        let scale = (frame.width() / w as f32)
            .min(frame.height() / h as f32)
            .min(1.0);
        let img = Rect::from_center_size(frame.center(), egui::vec2(w as f32, h as f32) * scale);
        painter.rect_filled(frame, 6.0, p.surface_hover);
        painter.image(
            tex.id(),
            img,
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        painter.rect_stroke(frame, 6.0, Stroke::new(1.0_f32, p.border));
        frame.right() + 12.0
    } else {
        painter.rect_filled(
            tile,
            8.0,
            if selected { p.surface } else { p.surface_hover },
        );
        painter.rect_stroke(tile, 8.0, Stroke::new(1.0_f32, p.border));
        row.kind.paint(
            &painter,
            tile.center(),
            if selected { p.accent } else { p.text_muted },
        );
        tile.right() + 12.0
    };

    // Texte.
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
            if del.hovered() {
                p.danger
            } else {
                p.text_faint
            },
        );
        if del.clicked() {
            action = RowAction::Delete;
        }

        let lock_rect = action_rect.translate(egui::vec2(-30.0, 0.0));
        let lock = (row.item_kind == ItemKind::Text).then(|| {
            ui.interact(lock_rect, resp.id.with("lock"), Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(if row.sensitive {
                    "Ce n'est pas un mot de passe : afficher en clair (⌘L)"
                } else {
                    "C'est un mot de passe : chiffrer et masquer (⌘L)"
                })
        });
        if let Some(lock) = lock {
            if lock.hovered() {
                painter.rect_filled(lock_rect, 7.0, p.accent.gamma_multiply(0.15));
            }
            icons::lock(
                &painter,
                lock_rect.center(),
                if lock.hovered() || row.sensitive {
                    p.accent
                } else {
                    p.text_faint
                },
            );
            if lock.clicked() {
                action = RowAction::ToggleSensitive;
            }
        }
    } else if selected {
        icons::enter(&painter, action_rect.center(), p.accent);
    }

    if matches!(action, RowAction::None) && resp.clicked() {
        action = RowAction::Copy;
    }
    resp.on_hover_ui(|ui| {
        ui.set_max_width(440.0);
        if let Some(tex) = thumb {
            let [w, h] = tex.size();
            let scale = (420.0 / w as f32).min(320.0 / h as f32).min(1.0);
            ui.add(egui::Image::new((
                tex.id(),
                egui::vec2(w as f32, h as f32) * scale,
            )));
            ui.label(RichText::new(&row.detail).size(11.5));
            return;
        }
        let text = if row.sensitive {
            "Mot de passe masqué — cliquez ou ↵ pour le copier".to_string()
        } else {
            tooltip(hist, row.index)
        };
        ui.label(RichText::new(text).size(12.5));
    });
    action
}

fn row_height(row: &Row) -> f32 {
    if row.thumb.is_some() {
        ROW_HEIGHT + 20.0
    } else {
        ROW_HEIGHT
    }
}

/// Ligne affichée pour un élément (aperçu, icône, statistiques, détails).
pub(super) fn build_row(i: usize, it: &history::Item) -> Row {
    let stats = it.stats();
    let mut row = Row {
        index: i,
        title: if it.sensitive {
            "••••••••••••".to_string()
        } else {
            history::preview(&it.text, PREVIEW_CHARS)
        },
        kind: if it.sensitive {
            Kind::Secret
        } else {
            Kind::detect(&it.text, stats)
        },
        sensitive: it.sensitive,
        copied_at: it.copied_at,
        lines: stats.lines,
        chars: stats.chars,
        item_kind: it.kind,
        thumb: None,
        detail: String::new(),
    };
    match it.kind {
        ItemKind::Text => {}
        ItemKind::Image => {
            row.kind = Kind::Image;
            row.title = "Image".into();
            row.thumb = Some(it.text.clone());
            row.detail = it.meta.clone();
        }
        ItemKind::Files => {
            let paths = it.paths();
            // macOS fournit les noms de fichiers décomposés (NFD : « e » +
            // accent) ; on les recompose pour un affichage correct.
            let name = |p: &std::path::Path| {
                p.file_name()
                    .map(|n| n.to_string_lossy().nfc().collect::<String>())
                    .unwrap_or_else(|| p.display().to_string().nfc().collect())
            };
            row.title = match paths.len() {
                0 => "Fichier".into(),
                1 => name(&paths[0]),
                n => format!("{} + {} autre(s)", name(&paths[0]), n - 1),
            };
            row.kind = if paths.len() == 1 && paths[0].is_dir() {
                Kind::Folder
            } else {
                Kind::File
            };
            let parent = paths
                .first()
                .and_then(|p| p.parent())
                .map(shorten_home)
                .unwrap_or_default();
            let missing = !paths.iter().all(|p| p.exists());
            row.detail = if missing {
                format!("{parent}  ·  introuvable")
            } else if paths.len() > 1 {
                format!("{} fichiers  ·  {parent}", paths.len())
            } else {
                parent
            };
        }
    }
    row
}

/// Empreintes des images encore présentes dans l'historique.
pub(super) fn image_hashes(history: &History) -> HashSet<&str> {
    history
        .items()
        .iter()
        .filter(|i| i.kind == ItemKind::Image)
        .map(|i| i.text.as_str())
        .collect()
}

/// Charge la vignette d'une image enregistrée en texture egui.
pub(super) fn load_thumbnail(ctx: &egui::Context, hash: &str) -> Option<egui::TextureHandle> {
    let path = media::thumb_path(&media::images_dir(), hash);
    match media::load_rgba(&path) {
        Ok((w, h, rgba)) => {
            let img = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
            Some(ctx.load_texture(format!("thumb-{hash}"), img, egui::TextureOptions::LINEAR))
        }
        Err(e) => {
            log::warn!("vignette illisible : {e}");
            None
        }
    }
}

/// Remplace le dossier personnel par « ~ ».
fn shorten_home(dir: &std::path::Path) -> String {
    let s: String = dir.display().to_string().nfc().collect();
    match directories::BaseDirs::new() {
        Some(b) => {
            let home = b.home_dir().display().to_string();
            match s.strip_prefix(&home) {
                Some(rest) => format!("~{rest}"),
                None => s,
            }
        }
        None => s,
    }
}

fn meta_text(row: &Row, now: u64) -> String {
    if row.item_kind != ItemKind::Text {
        let when = history::relative_time(row.copied_at, now);
        return if row.detail.is_empty() {
            when
        } else {
            format!("{when}  ·  {}", row.detail)
        };
    }
    if row.sensitive {
        return format!(
            "mot de passe  ·  {}",
            history::relative_time(row.copied_at, now)
        );
    }
    let mut parts = vec![history::relative_time(row.copied_at, now)];
    if row.lines > 1 {
        parts.push(format!("{} lignes", row.lines));
    }
    parts.push(if row.chars < 1000 {
        format!("{} car.", row.chars)
    } else {
        // Virgule décimale sur le nombre seul (pas sur « car. »).
        let k = format!("{:.1}", row.chars as f32 / 1000.0).replace('.', ",");
        format!("{k} k car.")
    });
    parts.join("  ·  ")
}

/// Texte complet (tronqué) affiché au survol d'un élément.
fn tooltip(history: &Mutex<History>, index: usize) -> String {
    let h = history.lock_safe();
    h.items()
        .get(index)
        .map(|it| {
            let mut s: String = it.text.chars().take(TOOLTIP_CHARS).collect();
            if it.text.chars().nth(TOOLTIP_CHARS).is_some() {
                s.push_str("\n…");
            }
            s
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::Item;

    fn text_row(text: &str) -> Row {
        build_row(0, &Item::text(text, 0, false))
    }

    #[test]
    fn kind_is_detected_from_the_text() {
        assert_eq!(text_row("https://exemple.fr/a").kind, Kind::Link);
        assert_eq!(text_row("  ~/Projets/copyq ").kind, Kind::Path);
        assert_eq!(text_row("amine@exemple.fr").kind, Kind::Email);
        assert_eq!(text_row("fn main() {\n}").kind, Kind::Multiline);
        assert_eq!(text_row("un texte").kind, Kind::Text);
        assert_eq!(text_row("voir https://exemple.fr").kind, Kind::Text);
        let secret = build_row(0, &Item::text("Xk9#mL2$pQ8!", 0, true));
        assert_eq!(secret.kind, Kind::Secret);
        assert!(!secret.title.contains("Xk9"), "jamais affiché en clair");
    }

    #[test]
    fn meta_line_summarises_the_item() {
        let now = 10_000;
        let mut row = text_row("une\ndeux\ntrois");
        row.copied_at = now - 120;
        assert_eq!(
            meta_text(&row, now),
            "il y a 2 min  ·  3 lignes  ·  14 car."
        );
        let long = text_row(&"x".repeat(2_500));
        assert_eq!(meta_text(&long, 0), "à l'instant  ·  2,5 k car.");
        let mut secret = build_row(0, &Item::text("secret", now, true));
        secret.copied_at = now;
        assert_eq!(meta_text(&secret, now), "mot de passe  ·  à l'instant");
    }

    #[test]
    fn files_row_shows_name_count_and_missing_state() {
        let item = Item {
            text: "/nulle-part/Présentation.mov\n/nulle-part/son.m4a".into(),
            kind: ItemKind::Files,
            ..Default::default()
        };
        let row = build_row(3, &item);
        assert_eq!(row.index, 3);
        assert_eq!(row.title, "Présentation.mov + 1 autre(s)");
        assert_eq!(row.kind, Kind::File);
        assert!(row.detail.ends_with("introuvable"), "{}", row.detail);
    }

    #[test]
    fn image_hashes_lists_only_images_still_in_history() {
        let mut h = History::default();
        let img = |x: &str| Item {
            text: x.into(),
            kind: ItemKind::Image,
            ..Default::default()
        };
        h.push_item(img("aaa"));
        h.push_item(img("bbb"));
        h.push_item(Item {
            text: "ccc".into(),
            ..Default::default()
        });
        let live = image_hashes(&h);
        assert_eq!(live, HashSet::from(["aaa", "bbb"]));
        let pos = h.items().iter().position(|i| i.text == "aaa").unwrap();
        h.remove(pos);
        assert_eq!(image_hashes(&h), HashSet::from(["bbb"]));
    }
}
