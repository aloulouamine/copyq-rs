//! Thème visuel moderne : palettes clair / sombre (suivant macOS), police système,
//! coins arrondis, espacements généreux.

use eframe::egui::{self, Color32, FontFamily, FontId, Rounding, Stroke, TextStyle};

/// Couleurs utilisées par l'interface (dessin personnalisé des lignes, puces…).
#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Color32,
    pub surface: Color32,
    pub surface_hover: Color32,
    pub border: Color32,
    pub text: Color32,
    pub text_muted: Color32,
    pub text_faint: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub danger: Color32,
    pub success: Color32,
}

pub const DARK: Palette = Palette {
    bg: Color32::from_rgb(0x17, 0x17, 0x1A),
    surface: Color32::from_rgb(0x22, 0x22, 0x27),
    surface_hover: Color32::from_rgb(0x2A, 0x2A, 0x31),
    border: Color32::from_rgb(0x32, 0x32, 0x3A),
    text: Color32::from_rgb(0xED, 0xED, 0xF0),
    text_muted: Color32::from_rgb(0x9A, 0x9A, 0xA6),
    text_faint: Color32::from_rgb(0x6B, 0x6B, 0x76),
    accent: Color32::from_rgb(0x8B, 0x7C, 0xF6),
    accent_soft: Color32::from_rgb(0x2E, 0x2A, 0x4A),
    danger: Color32::from_rgb(0xF2, 0x6D, 0x6D),
    success: Color32::from_rgb(0x4A, 0xD6, 0x8F),
};

pub const LIGHT: Palette = Palette {
    bg: Color32::from_rgb(0xF7, 0xF7, 0xF9),
    surface: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    surface_hover: Color32::from_rgb(0xEF, 0xEF, 0xF3),
    border: Color32::from_rgb(0xE2, 0xE2, 0xE8),
    text: Color32::from_rgb(0x1C, 0x1C, 0x21),
    text_muted: Color32::from_rgb(0x6B, 0x6B, 0x76),
    text_faint: Color32::from_rgb(0x9A, 0x9A, 0xA6),
    accent: Color32::from_rgb(0x6D, 0x5B, 0xF0),
    accent_soft: Color32::from_rgb(0xEC, 0xE9, 0xFE),
    danger: Color32::from_rgb(0xD9, 0x3F, 0x3F),
    success: Color32::from_rgb(0x1F, 0xA8, 0x63),
};

pub fn palette_ctx(ctx: &egui::Context) -> Palette {
    if ctx.style().visuals.dark_mode {
        DARK
    } else {
        LIGHT
    }
}

/// Installe polices et styles (à appeler une fois au démarrage).
pub fn install(ctx: &egui::Context) {
    install_fonts(ctx);
    ctx.set_style_of(egui::Theme::Dark, style(egui::Visuals::dark(), &DARK));
    ctx.set_style_of(egui::Theme::Light, style(egui::Visuals::light(), &LIGHT));
}

fn style(mut v: egui::Visuals, p: &Palette) -> egui::Style {
    let r = Rounding::same(8.0);

    v.panel_fill = p.bg;
    v.window_fill = p.surface;
    v.extreme_bg_color = p.surface;
    v.faint_bg_color = p.surface;
    v.window_rounding = Rounding::same(12.0);
    v.window_stroke = Stroke::new(1.0, p.border);
    v.menu_rounding = r;
    v.hyperlink_color = p.accent;
    v.override_text_color = None;
    v.selection.bg_fill = p.accent.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.text_cursor.stroke = Stroke::new(2.0, p.accent);

    for (w, fill) in [
        (&mut v.widgets.noninteractive, p.bg),
        (&mut v.widgets.inactive, p.surface),
        (&mut v.widgets.hovered, p.surface_hover),
        (&mut v.widgets.active, p.surface_hover),
        (&mut v.widgets.open, p.surface_hover),
    ] {
        w.rounding = r;
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.bg_stroke = Stroke::NONE;
        w.expansion = 0.0;
    }
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.active.fg_stroke = Stroke::new(1.0, p.text);

    let mut s = egui::Style {
        visuals: v,
        ..Default::default()
    };
    s.spacing.item_spacing = egui::vec2(8.0, 6.0);
    s.spacing.button_padding = egui::vec2(10.0, 5.0);
    s.spacing.interact_size.y = 24.0;
    s.spacing.scroll = egui::style::ScrollStyle::floating();
    s.text_styles = [
        (TextStyle::Small, FontId::new(11.0, FontFamily::Proportional)),
        (TextStyle::Body, FontId::new(13.5, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(13.0, FontFamily::Proportional)),
        (TextStyle::Heading, FontId::new(18.0, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(12.5, FontFamily::Monospace)),
    ]
    .into();
    s
}

/// Utilise la police système de macOS (SF Pro) si elle est disponible,
/// en gardant les polices d'egui en secours (emoji, symboles).
fn install_fonts(ctx: &egui::Context) {
    let candidates = [
        "/System/Library/Fonts/SFNS.ttf",
        "/System/Library/Fonts/SFNSText.ttf",
        "/Library/Fonts/SF-Pro-Text-Regular.otf",
    ];
    let Some(bytes) = candidates.iter().find_map(|p| std::fs::read(p).ok()) else {
        return;
    };
    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert("system".to_owned(), egui::FontData::from_owned(bytes));
    if let Some(family) = fonts.families.get_mut(&FontFamily::Proportional) {
        family.insert(0, "system".to_owned());
    }
    ctx.set_fonts(fonts);
}
