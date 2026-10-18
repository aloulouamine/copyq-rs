//! Thème visuel moderne : palettes clair / sombre (suivant macOS), police système,
//! coins arrondis, espacements généreux.

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle};

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
    if ctx.global_style().visuals.dark_mode {
        DARK
    } else {
        LIGHT
    }
}

/// Installe polices et styles (à appeler une fois au démarrage).
pub fn install(ctx: &egui::Context, lang: crate::i18n::Lang) {
    install_fonts(ctx, lang);
    ctx.set_style_of(egui::Theme::Dark, style(egui::Visuals::dark(), &DARK));
    ctx.set_style_of(egui::Theme::Light, style(egui::Visuals::light(), &LIGHT));

    // Forcer un thème : COPYQ_THEME=dark ou COPYQ_THEME=light (sinon : celui de macOS).
    match std::env::var("COPYQ_THEME").as_deref() {
        Ok("dark") => ctx.set_theme(egui::Theme::Dark),
        Ok("light") => ctx.set_theme(egui::Theme::Light),
        _ => {}
    }
}

fn style(mut v: egui::Visuals, p: &Palette) -> egui::Style {
    let r = CornerRadius::same(8);

    v.panel_fill = p.bg;
    v.window_fill = p.surface;
    v.extreme_bg_color = p.surface;
    v.faint_bg_color = p.surface;
    v.window_corner_radius = CornerRadius::same(12);
    v.window_stroke = Stroke::new(1.0_f32, p.border);
    v.menu_corner_radius = r;
    v.hyperlink_color = p.accent;
    v.override_text_color = None;
    v.selection.bg_fill = p.accent.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0_f32, p.accent);
    v.text_cursor.stroke = Stroke::new(2.0_f32, p.accent);

    for (w, fill) in [
        (&mut v.widgets.noninteractive, p.bg),
        (&mut v.widgets.inactive, p.surface),
        (&mut v.widgets.hovered, p.surface_hover),
        (&mut v.widgets.active, p.surface_hover),
        (&mut v.widgets.open, p.surface_hover),
    ] {
        w.corner_radius = r;
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.bg_stroke = Stroke::NONE;
        w.expansion = 0.0;
    }
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, p.text);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, p.border);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, p.text);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, p.text);
    v.widgets.active.fg_stroke = Stroke::new(1.0_f32, p.text);

    let mut s = egui::Style {
        visuals: v,
        ..Default::default()
    };
    s.spacing.item_spacing = egui::vec2(8.0, 6.0);
    s.spacing.button_padding = egui::vec2(10.0, 5.0);
    s.spacing.interact_size.y = 24.0;
    s.spacing.scroll = egui::style::ScrollStyle::floating();
    s.text_styles = [
        (
            TextStyle::Small,
            FontId::new(11.0, FontFamily::Proportional),
        ),
        (TextStyle::Body, FontId::new(13.5, FontFamily::Proportional)),
        (
            TextStyle::Button,
            FontId::new(13.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Heading,
            FontId::new(18.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new(12.5, FontFamily::Monospace),
        ),
    ]
    .into();
    s
}

/// Utilise la police système de macOS (SF Pro) si elle est disponible, la police arabe
/// du système en secours, plus une police chinoise, japonaise ou coréenne quand
/// l'interface est dans l'une de ces langues ; les polices d'egui restent en secours
/// (emoji, symboles).
pub fn install_fonts(ctx: &egui::Context, lang: crate::i18n::Lang) {
    let mut fonts = egui::FontDefinitions::default();
    let mut first = 0;
    let mut add = |fonts: &mut egui::FontDefinitions, name: &str, bytes: Vec<u8>| {
        fonts.font_data.insert(
            name.to_owned(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
        if let Some(family) = fonts.families.get_mut(&FontFamily::Proportional) {
            family.insert(first, name.to_owned());
            first += 1;
        }
    };
    let system = [
        "/System/Library/Fonts/SFNS.ttf",
        "/System/Library/Fonts/SFNSText.ttf",
        "/Library/Fonts/SF-Pro-Text-Regular.otf",
    ];
    if let Some(bytes) = system.iter().find_map(|p| std::fs::read(p).ok()) {
        add(&mut fonts, "system", bytes);
    }
    // Arabe : police système en secours pour tout le monde (sans elle, un texte
    // arabe copié s'affiche en carrés vides), et pour l'interface en arabe.
    let arabic = [
        "/System/Library/Fonts/SFArabic.ttf",
        "/System/Library/Fonts/GeezaPro.ttc",
        "/System/Library/Fonts/Supplemental/GeezaPro.ttc",
    ];
    if let Some(bytes) = arabic.iter().find_map(|p| std::fs::read(p).ok()) {
        add(&mut fonts, "arabic", bytes);
    } else {
        log::warn!("police arabe introuvable : le texte arabe s'affichera mal");
    }
    let cjk = cjk_font_candidates(lang);
    match cjk
        .iter()
        .find_map(|p| std::fs::read(p).ok().map(|b| (p, b)))
    {
        Some((path, bytes)) => {
            log::info!("police {} : {path}", lang.code());
            add(&mut fonts, "cjk", bytes);
        }
        None if lang.is_cjk() => {
            log::warn!(
                "aucune police système pour {} : caractères manquants",
                lang.code()
            )
        }
        None => {}
    }
    ctx.set_fonts(fonts);
}

/// Polices système (macOS) pour les écritures chinoise, japonaise et coréenne.
fn cjk_font_candidates(lang: crate::i18n::Lang) -> &'static [&'static str] {
    use crate::i18n::Lang;
    match lang {
        Lang::Ja => &[
            "/System/Library/Fonts/ヒラギノ角ゴシック W4.ttc",
            "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
            "/System/Library/Fonts/Hiragino Sans GB.ttc",
            "/System/Library/Fonts/PingFang.ttc",
        ],
        Lang::Ko => &[
            "/System/Library/Fonts/AppleSDGothicNeo.ttc",
            "/System/Library/Fonts/Supplemental/AppleGothic.ttf",
        ],
        Lang::ZhHans => &[
            "/System/Library/Fonts/PingFang.ttc",
            "/System/Library/Fonts/Hiragino Sans GB.ttc",
            "/System/Library/Fonts/STHeiti Light.ttc",
            "/System/Library/Fonts/STHeiti Medium.ttc",
        ],
        Lang::ZhHant => &[
            "/System/Library/Fonts/PingFang.ttc",
            "/System/Library/Fonts/STHeiti Light.ttc",
            "/System/Library/Fonts/STHeiti Medium.ttc",
            "/System/Library/Fonts/Hiragino Sans GB.ttc",
        ],
        _ => &[],
    }
}
