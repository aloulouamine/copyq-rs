//! Petits composants d'interface réutilisables (interrupteur, touches, légendes…).

use crate::theme::Palette;
use eframe::egui::{
    self, text::LayoutJob, text::TextWrapping, Color32, CornerRadius, FontId, Frame, Margin,
    RichText, Sense, Stroke, Ui,
};
use std::sync::Arc;

pub(super) fn single_line(
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
    ui.painter().layout_job(job)
}

/// Ligne horizontale dans le sens de lecture : de droite à gauche en arabe.
pub(super) fn hstack<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
    if crate::i18n::is_rtl() {
        let size = egui::vec2(ui.available_width(), ui.spacing().interact_size.y);
        ui.allocate_ui_with_layout(size, egui::Layout::right_to_left(egui::Align::Center), add)
    } else {
        ui.horizontal(add)
    }
}

/// Comme `hstack`, avec retour à la ligne.
pub(super) fn hwrap<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
    if crate::i18n::is_rtl() {
        let size = egui::vec2(ui.available_width(), ui.spacing().interact_size.y);
        let layout = egui::Layout::right_to_left(egui::Align::Center).with_main_wrap(true);
        ui.allocate_ui_with_layout(size, layout, add)
    } else {
        ui.horizontal_wrapped(add)
    }
}

/// Disposition des éléments poussés au bout de la ligne (à droite, à gauche en arabe).
pub(super) fn trailing() -> egui::Layout {
    if crate::i18n::is_rtl() {
        egui::Layout::left_to_right(egui::Align::Center)
    } else {
        egui::Layout::right_to_left(egui::Align::Center)
    }
}

/// Colonne alignée sur le début de la ligne (à droite en arabe, textes compris).
pub(super) fn column() -> egui::Layout {
    egui::Layout::top_down(if crate::i18n::is_rtl() {
        egui::Align::Max
    } else {
        egui::Align::Min
    })
}

pub(super) fn empty_state(ui: &mut Ui, history_empty: bool, query: &str, p: &Palette) {
    ui.vertical_centered(|ui| {
        ui.add_space((ui.available_height() / 2.0 - 60.0).max(20.0));
        let (icon, title, subtitle) = if history_empty {
            (
                "📋",
                t!("empty.title").to_string(),
                t!("empty.subtitle").to_string(),
            )
        } else {
            (
                "🔍",
                t!("empty.no_results").to_string(),
                t!("empty.no_match", query = query),
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
pub(super) fn toggle(ui: &mut Ui, on: &mut bool, label: &str, p: &Palette) -> egui::Response {
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
    ui.painter()
        .rect_filled(rect, CornerRadius::same(radius as u8), track);
    let x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), t);
    ui.painter()
        .circle_filled(egui::pos2(x, rect.center().y), radius - 2.5, Color32::WHITE);
    resp | label_resp
}

pub(super) fn caption(ui: &mut Ui, text: &str, p: &Palette) {
    paragraph(ui, text, 11.5, p.text_faint);
}

/// Texte sur plusieurs lignes au besoin.
///
/// `text` est dans l'ordre logique (`tp!`, texte de l'utilisateur). Un texte de droite à
/// gauche est coupé ici en lignes, dans l'ordre logique, puis chaque ligne est mise
/// dans l'ordre visuel (`i18n::visual`) et affichée seule : egui, lui, couperait le
/// paragraphe déjà retourné, et la fin de la phrase se retrouverait en haut.
pub(super) fn paragraph(ui: &mut Ui, text: &str, size: f32, color: Color32) {
    if !crate::i18n::starts_rtl(text) {
        ui.add(egui::Label::new(RichText::new(text).size(size).color(color)).wrap());
        return;
    }
    let font = FontId::proportional(size);
    let width = ui.available_width().max(40.0);
    let ctx = ui.ctx().clone();
    let measure = |line: &str| {
        ctx.fonts_mut(|f| {
            f.layout_no_wrap(crate::i18n::visual(line), font.clone(), color)
                .size()
                .x
        })
    };
    let lines = break_lines(text, width, measure);
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for line in lines {
            ui.add(
                egui::Label::new(
                    RichText::new(crate::i18n::visual(&line))
                        .size(size)
                        .color(color),
                )
                .wrap_mode(egui::TextWrapMode::Extend),
            );
        }
    });
}

/// Coupe un texte en lignes, dans l'ordre logique, aux espaces, sans dépasser
/// `width` (un mot plus long qu'une ligne reste seul sur la sienne). Les retours à la
/// ligne du texte sont conservés.
pub(super) fn break_lines(text: &str, width: f32, measure: impl Fn(&str) -> f32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split(' ').filter(|w| !w.is_empty()) {
            let candidate = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if !line.is_empty() && measure(&candidate) > width {
                lines.push(std::mem::replace(&mut line, word.to_string()));
            } else {
                line = candidate;
            }
        }
        lines.push(line);
    }
    lines
}

pub(super) fn ghost_button(ui: &mut Ui, text: &str, color: Color32, p: &Palette) -> egui::Response {
    let _ = p;
    ui.add(
        egui::Button::new(RichText::new(text).size(12.5).color(color))
            .fill(Color32::TRANSPARENT)
            .corner_radius(7.0),
    )
}

/// Petite touche de clavier stylisée.
pub(super) fn kbd(ui: &mut Ui, key: &str, p: &Palette) {
    Frame::NONE
        .fill(p.surface)
        .stroke(Stroke::new(1.0_f32, p.border))
        .corner_radius(4.0)
        .inner_margin(Margin::symmetric(5, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(key).size(11.0).color(p.text_muted));
        });
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_are_broken_in_logical_order() {
        // Largeur : 1 par caractère.
        let w = |s: &str| s.chars().count() as f32;
        assert_eq!(
            break_lines("un deux trois quatre", 9.0, w),
            vec!["un deux", "trois", "quatre"]
        );
        assert_eq!(break_lines("a\nb c", 10.0, w), vec!["a", "b c"]);
        assert_eq!(
            break_lines("immense", 3.0, w),
            vec!["immense"],
            "mot trop long"
        );
        let ar = "عند تجاوزه، يحذف الأقدم.";
        let lines = break_lines(ar, 12.0, w);
        assert_eq!(lines.join(" "), ar, "rien de perdu ni déplacé");
        assert_eq!(lines[0], "عند تجاوزه،", "le début de la phrase en premier");
    }

    /// Sur un Mac (police arabe du système) : chaque ligne affichée par egui, lue de
    /// droite à gauche, redonne bien le texte dans l'ordre.
    #[test]
    fn arabic_lines_read_right_to_left_in_order() {
        let Some(bytes) = [
            "/System/Library/Fonts/SFArabic.ttf",
            "/System/Library/Fonts/GeezaPro.ttc",
        ]
        .iter()
        .find_map(|p| std::fs::read(p).ok()) else {
            return;
        };
        let ctx = egui::Context::default();
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "ar".into(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
        fonts
            .families
            .get_mut(&egui::FontFamily::Proportional)
            .unwrap()
            .insert(0, "ar".into());
        ctx.set_fonts(fonts);
        let mut frame = ctx.run_ui(Default::default(), |_| {});
        frame.textures_delta.clear();
        let font = FontId::proportional(14.0);
        let measure = |s: &str| {
            ctx.fonts_mut(|f| {
                f.layout_no_wrap(crate::i18n::visual(s), font.clone(), Color32::WHITE)
                    .size()
                    .x
            })
        };
        let text = "يلصق العنصر المختار مباشرة في التطبيق الذي فتحت منه السجل.";
        let lines = break_lines(text, 160.0, measure);
        assert!(lines.len() > 1, "{lines:?}");
        let mut read = Vec::new();
        for line in &lines {
            let galley = ctx.fonts_mut(|f| {
                f.layout_no_wrap(crate::i18n::visual(line), font.clone(), Color32::WHITE)
            });
            let mut glyphs: Vec<(char, f32)> = galley
                .rows
                .iter()
                .flat_map(|r| r.glyphs.iter())
                .filter(|g| g.advance_width > 0.01 || g.chr == ' ')
                .map(|g| (g.chr, g.pos.x))
                .collect();
            glyphs.sort_by(|a, b| b.1.total_cmp(&a.1));
            read.push(
                glyphs
                    .iter()
                    .map(|g| g.0)
                    .collect::<String>()
                    .trim()
                    .to_string(),
            );
        }
        assert_eq!(
            read.join(" "),
            text,
            "lignes lues de droite à gauche : {read:?}"
        );
    }

    /// Toutes les traductions arabes (sur une ou plusieurs lignes) : les lettres arabes
    /// lues de droite à gauche sont dans l'ordre logique. Chiffres, latin et signes
    /// diacritiques sont ignorés (ils ont leur propre sens de lecture ou pas de largeur).
    #[test]
    fn every_arabic_translation_reads_in_order() {
        let Some(bytes) = [
            "/System/Library/Fonts/SFArabic.ttf",
            "/System/Library/Fonts/GeezaPro.ttc",
        ]
        .iter()
        .find_map(|p| std::fs::read(p).ok()) else {
            return;
        };
        let ctx = egui::Context::default();
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "ar".into(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
        fonts
            .families
            .get_mut(&egui::FontFamily::Proportional)
            .unwrap()
            .insert(0, "ar".into());
        ctx.set_fonts(fonts);
        let mut frame = ctx.run_ui(Default::default(), |_| {});
        frame.textures_delta.clear();
        let font = FontId::proportional(12.0);
        let layout = |s: &str| {
            ctx.fonts_mut(|f| {
                f.layout_no_wrap(crate::i18n::visual(s), font.clone(), Color32::WHITE)
            })
        };
        let letters = |s: &str| -> String {
            s.chars()
                .filter(|c| matches!(*c as u32, 0x0621..=0x064A))
                .collect()
        };
        let ar: std::collections::HashMap<String, String> =
            serde_json::from_str(include_str!("../../locales/ar.json")).unwrap();
        let mut multi_line = 0;
        for (key, text) in &ar {
            let lines = break_lines(text, 200.0, |s| layout(s).size().x);
            multi_line += usize::from(lines.len() > 1);
            let mut read = String::new();
            for line in &lines {
                let galley = layout(line);
                let mut glyphs: Vec<(char, f32)> = galley
                    .rows
                    .iter()
                    .flat_map(|r| r.glyphs.iter())
                    .filter(|g| g.advance_width > 0.01)
                    .map(|g| (g.chr, g.pos.x))
                    .collect();
                glyphs.sort_by(|a, b| b.1.total_cmp(&a.1));
                read.extend(glyphs.iter().map(|g| g.0));
            }
            assert_eq!(
                letters(&read),
                letters(text),
                "{key} : « {text} » mal ordonné (lignes {lines:?})"
            );
        }
        assert!(
            multi_line >= 5,
            "des textes sur plusieurs lignes sont testés"
        );
    }
}
