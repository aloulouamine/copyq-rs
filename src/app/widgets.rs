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
    ui.add(egui::Label::new(RichText::new(text).size(11.5).color(p.text_faint)).wrap());
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
    use eframe::egui;

    /// Diagnostic : position horizontale de chaque caractère tel qu'egui le dessine.
    fn glyph_xs(text: &str) -> Option<Vec<(char, f32)>> {
        let bytes = [
            "/System/Library/Fonts/SFArabic.ttf",
            "/System/Library/Fonts/GeezaPro.ttc",
        ]
        .iter()
        .find_map(|p| std::fs::read(p).ok())?;
        let ctx = egui::Context::default();
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "arabic".into(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
        fonts
            .families
            .get_mut(&egui::FontFamily::Proportional)
            .unwrap()
            .insert(0, "arabic".into());
        ctx.set_fonts(fonts);
        let _ = ctx.run_ui(Default::default(), |_| {});
        let galley = ctx.fonts_mut(|f| {
            f.layout_no_wrap(
                text.to_owned(),
                egui::FontId::proportional(14.0),
                egui::Color32::WHITE,
            )
        });
        Some(
            galley
                .rows
                .iter()
                .flat_map(|r| r.glyphs.iter().map(|g| (g.chr, g.pos.x)))
                .collect(),
        )
    }

    #[test]
    fn diagnostic_bidi() {
        let mut out = String::new();
        for text in ["مسح الكل", "\u{200F}مسح الكل.\u{200F}", "abc def"] {
            let Some(xs) = glyph_xs(text) else { return };
            let xs: Vec<String> = xs.iter().map(|(c, x)| format!("{c}@{x:.0}")).collect();
            out += &format!("\n{text:?} => {}", xs.join(" "));
        }
        panic!("DIAG{out}");
    }
}
