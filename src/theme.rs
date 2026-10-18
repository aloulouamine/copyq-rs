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
    let mut add = |fonts: &mut egui::FontDefinitions, name: &str, bytes: Vec<u8>, index: u32| {
        let mut data = egui::FontData::from_owned(bytes);
        // Fichier .ttc : plusieurs polices, on choisit la bonne (ex. PingFang SC/TC/HK).
        data.index = index;
        fonts
            .font_data
            .insert(name.to_owned(), std::sync::Arc::new(data));
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
        add(&mut fonts, "system", bytes, 0);
    }
    // Arabe : police système en secours pour tout le monde (sans elle, un texte
    // arabe copié s'affiche en carrés vides), et pour l'interface en arabe.
    let arabic = [
        "/System/Library/Fonts/SFArabic.ttf",
        "/System/Library/Fonts/GeezaPro.ttc",
        "/System/Library/Fonts/Supplemental/GeezaPro.ttc",
    ];
    if let Some(bytes) = arabic.iter().find_map(|p| std::fs::read(p).ok()) {
        add(&mut fonts, "arabic", bytes, 0);
    } else {
        log::warn!("police arabe introuvable : le texte arabe s'affichera mal");
    }
    let cjk = cjk_font_candidates(lang);
    match cjk
        .iter()
        .find_map(|(p, i)| std::fs::read(p).ok().map(|b| (p, *i, b)))
    {
        Some((path, index, bytes)) => {
            log::info!("police {} : {path} (n° {index})", lang.code());
            add(&mut fonts, "cjk", bytes, index);
        }
        None if lang.is_cjk() => {
            log::warn!(
                "aucune police système pour {} : caractères manquants",
                lang.code()
            )
        }
        None => {}
    }
    // Famille « rtl » (textes arabes, voir `app::widgets::bidi_job`) : la police arabe
    // d'abord, pour qu'un mot arabe, ses espaces et sa ponctuation forment un seul bloc.
    let mut rtl = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    if let Some(pos) = rtl.iter().position(|f| f == "arabic") {
        let arabic = rtl.remove(pos);
        rtl.insert(0, arabic);
    }
    fonts
        .families
        .insert(FontFamily::Name(crate::app::RTL_FAMILY.into()), rtl);
    ctx.set_fonts(fonts);
}

/// Polices système (macOS) pour les écritures chinoise, japonaise et coréenne :
/// (fichier, n° de la police dans un .ttc). Les caractères chinois partagés s'écrivent
/// différemment en Chine, à Taïwan, à Hong Kong et au Japon : il faut la police de la
/// région (PingFang.ttc : 0 = HK, 1 = TC, 2 = SC ; STHeiti Light.ttc : 0 = TC, 1 = SC ;
/// vérifié par `cjk_fonts_match_the_region` et `ci/ttc_faces.py`).
pub(crate) fn cjk_font_candidates(lang: crate::i18n::Lang) -> &'static [(&'static str, u32)] {
    use crate::i18n::Lang;
    match lang {
        Lang::Ja => &[
            ("/System/Library/Fonts/ヒラギノ角ゴシック W4.ttc", 0),
            ("/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc", 0),
        ],
        Lang::Ko => &[
            ("/System/Library/Fonts/AppleSDGothicNeo.ttc", 0),
            ("/System/Library/Fonts/Supplemental/AppleGothic.ttf", 0),
        ],
        Lang::ZhHans => &[
            ("/System/Library/Fonts/PingFang.ttc", 2),
            ("/System/Library/Fonts/STHeiti Light.ttc", 1),
            ("/System/Library/Fonts/Hiragino Sans GB.ttc", 0),
        ],
        Lang::ZhHant => &[
            ("/System/Library/Fonts/PingFang.ttc", 1),
            ("/System/Library/Fonts/STHeiti Light.ttc", 0),
        ],
        _ => &[],
    }
}

/// Nom complet (table `name`, anglais) de la police n° `index` d'un fichier .ttf/.ttc.
#[cfg(test)]
pub(crate) fn font_full_name(data: &[u8], index: u32) -> Option<String> {
    let u16_at = |o: usize| Some(u16::from_be_bytes(data.get(o..o + 2)?.try_into().ok()?));
    let u32_at = |o: usize| Some(u32::from_be_bytes(data.get(o..o + 4)?.try_into().ok()?));
    let face = if data.get(0..4)? == b"ttcf" {
        u32_at(12 + 4 * index as usize)? as usize
    } else {
        0
    };
    let tables = u16_at(face + 4)? as usize;
    for t in 0..tables {
        let rec = face + 12 + 16 * t;
        if data.get(rec..rec + 4)? != b"name" {
            continue;
        }
        let name = u32_at(rec + 8)? as usize;
        let count = u16_at(name + 2)? as usize;
        let strings = name + u16_at(name + 4)? as usize;
        let mut fallback = None;
        for r in 0..count {
            let n = name + 6 + 12 * r;
            let (platform, lang_id, id) = (u16_at(n)?, u16_at(n + 4)?, u16_at(n + 6)?);
            let (len, off) = (u16_at(n + 8)? as usize, u16_at(n + 10)? as usize);
            let raw = data.get(strings + off..strings + off + len)?;
            if id == 4 && platform == 3 && lang_id == 0x409 {
                let utf16: Vec<u16> = raw
                    .chunks(2)
                    .map(|c| u16::from_be_bytes([c[0], c[1]]))
                    .collect();
                return String::from_utf16(&utf16).ok();
            }
            if id == 4 && platform == 1 {
                fallback = Some(String::from_utf8_lossy(raw).into_owned());
            }
        }
        return fallback;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Lang;

    /// Sur un Mac : la police choisie pour chaque langue est bien celle de la région.
    #[test]
    fn cjk_fonts_match_the_region() {
        if !std::path::Path::new("/System/Library/Fonts/PingFang.ttc").exists() {
            return;
        }
        for (lang, expected) in [
            (Lang::ZhHans, "SC"),
            (Lang::ZhHant, "TC"),
            (Lang::Ja, "Hiragino Sans"),
            (Lang::Ko, "Gothic"),
        ] {
            let (path, index) = cjk_font_candidates(lang)
                .iter()
                .find(|(p, _)| std::path::Path::new(p).exists())
                .unwrap_or_else(|| panic!("{} : aucune police", lang.code()));
            let data = std::fs::read(path).unwrap();
            let name = font_full_name(&data, *index).unwrap_or_default();
            assert!(
                name.contains(expected),
                "{} : {path} n° {index} = « {name} », attendu « {expected} »",
                lang.code()
            );
        }
    }
}
