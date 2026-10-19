//! Petits composants d'interface réutilisables (interrupteur, touches, légendes…), et
//! affichage des textes de droite à gauche (arabe).

use crate::i18n;
use crate::theme::Palette;
use eframe::egui::{
    self, text::LayoutJob, text::TextWrapping, Color32, CornerRadius, FontFamily, FontId, Frame,
    Margin, RichText, Sense, Stroke, Ui, WidgetText,
};
use std::sync::Arc;

/// Famille de polices où la police arabe passe en premier (voir `theme`) : un mot arabe,
/// ses espaces et sa ponctuation arabe restent ainsi d'un seul tenant pour egui.
pub(crate) const RTL_FAMILY: &str = "rtl";

/// Faut-il composer ce texte de droite à gauche ?
fn needs_bidi(text: &str) -> bool {
    i18n::has_rtl(text)
}

/// Une ligne de texte prête pour egui : les morceaux (`i18n::visual_pieces`) dans
/// l'ordre visuel, l'arabe dans la famille `RTL_FAMILY`, le reste dans la police normale.
pub(super) fn bidi_job(text: &str, size: f32, color: Color32) -> LayoutJob {
    bidi_job_with_base(text, size, color, i18n::is_rtl() || i18n::starts_rtl(text))
}

fn bidi_job_with_base(text: &str, size: f32, color: Color32, rtl_base: bool) -> LayoutJob {
    let mut job = LayoutJob::default();
    for piece in i18n::visual_pieces(text, rtl_base) {
        let family = if piece.arabic {
            FontFamily::Name(RTL_FAMILY.into())
        } else {
            FontFamily::Proportional
        };
        job.append(
            &piece.text,
            0.0,
            egui::TextFormat::simple(FontId::new(size, family), color),
        );
    }
    job
}

/// Texte d'un widget (libellé, bouton, info-bulle…) : tel quel, ou composé de droite
/// à gauche s'il contient de l'arabe.
pub(super) fn wt(text: &str, size: f32, color: Color32) -> WidgetText {
    if needs_bidi(text) {
        bidi_job(text, size, color).into()
    } else {
        RichText::new(text).size(size).color(color).into()
    }
}

/// Une ligne, tronquée (« … ») si elle dépasse `max_width`.
pub(super) fn single_line(
    ui: &Ui,
    text: &str,
    size: f32,
    color: Color32,
    max_width: f32,
) -> Arc<egui::Galley> {
    if !needs_bidi(text) {
        let mut job = LayoutJob::single_section(
            text.to_owned(),
            egui::TextFormat::simple(FontId::proportional(size), color),
        );
        job.wrap = TextWrapping::truncate_at_width(max_width);
        return ui.painter().layout_job(job);
    }
    // De droite à gauche : on raccourcit le texte par la fin *logique* (egui couperait
    // la fin visuelle, c'est-à-dire le début de la phrase).
    let layout = |s: &str| ui.painter().layout_job(bidi_job(s, size, color));
    let full = layout(text);
    if full.size().x <= max_width {
        return full;
    }
    let chars: Vec<char> = text.chars().collect();
    let (mut lo, mut hi) = (0, chars.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let candidate: String = chars[..mid].iter().collect::<String>() + "…";
        if layout(&candidate).size().x <= max_width {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    layout(
        &(chars[..lo]
            .iter()
            .collect::<String>()
            .trim_end()
            .to_string()
            + "…"),
    )
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
        ui.label(wt(&title, 15.0, p.text));
        ui.add_space(2.0);
        ui.label(wt(&subtitle, 12.5, p.text_muted));
    });
}

/// Interrupteur façon iOS / macOS, avec libellé cliquable.
pub(super) fn toggle(ui: &mut Ui, on: &mut bool, label: &str, p: &Palette) -> egui::Response {
    let size = egui::vec2(30.0, 18.0);
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
    let label_resp = ui.add(egui::Label::new(wt(label, 12.5, p.text_muted)).sense(Sense::click()));
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
/// Un texte avec de l'arabe est coupé ici en lignes, dans l'ordre logique, et chaque
/// ligne est composée à part (`bidi_job`) : egui couperait le paragraphe tel qu'il le
/// pose, de gauche à droite, et la fin de la phrase se retrouverait en haut.
pub(super) fn paragraph(ui: &mut Ui, text: &str, size: f32, color: Color32) {
    if !needs_bidi(text) {
        ui.add(egui::Label::new(RichText::new(text).size(size).color(color)).wrap());
        return;
    }
    let width = ui.available_width().max(40.0);
    let painter = ui.painter().clone();
    let measure = |line: &str| painter.layout_job(bidi_job(line, size, color)).size().x;
    let lines = break_lines(text, width, measure);
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for line in lines {
            ui.add(egui::Label::new(wt(&line, size, color)).wrap_mode(egui::TextWrapMode::Extend));
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
        egui::Button::new(wt(text, 12.5, color))
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
            ui.label(wt(key, 11.0, p.text_muted));
        });
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

/// Fenêtre surgissante ancrée à un bouton qui l'ouvre et la ferme (Réglages).
///
/// La mémoire d'egui ne garde qu'une fenêtre surgissante ouverte à la fois : ouvrir
/// une liste déroulante à l'intérieur y fermait le panneau. L'état est donc gardé
/// ici, et un clic à l'extérieur ne ferme le panneau que si aucune liste déroulante
/// n'était ouverte (un clic dans la liste est « à l'extérieur » du panneau).
#[derive(Default)]
pub(super) struct PopupState {
    pub open: bool,
    sub_was_open: bool,
}

pub(super) fn anchored_popup<R>(
    ui: &mut Ui,
    button: &egui::Response,
    id: egui::Id,
    align: egui::RectAlign,
    state: &mut PopupState,
    add: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    let shown = egui::Popup::from_toggle_button_response(button)
        .id(id)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .align(align)
        .show(add);
    state.open = egui::Popup::is_id_open(ui.ctx(), id);
    let _ = &mut state.sub_was_open;
    let inner = shown.map(|s| s.inner);
    inner
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Simule des clics de souris sur une petite interface : un bouton « Réglages »
    /// ouvrant un panneau qui contient une liste déroulante.
    #[test]
    fn choosing_in_a_combo_box_keeps_the_panel_open() {
        struct Demo {
            state: PopupState,
            choice: usize,
            button: egui::Rect,
            combo: egui::Rect,
            panel: Option<egui::Rect>,
        }
        let ctx = egui::Context::default();
        let mut demo = Demo {
            state: PopupState::default(),
            choice: 0,
            button: egui::Rect::NOTHING,
            combo: egui::Rect::NOTHING,
            panel: None,
        };
        let mut items: Vec<egui::Rect> = Vec::new();
        let frame = |events: Vec<egui::Event>, demo: &mut Demo, items: &mut Vec<egui::Rect>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 600.0),
                )),
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                ui.add_space(300.0);
                let button = ui.button("Réglages");
                demo.button = button.rect;
                let mut combo = egui::Rect::NOTHING;
                let mut rows = Vec::new();
                let panel = anchored_popup(
                    ui,
                    &button,
                    egui::Id::unique("demo_panel"),
                    egui::RectAlign::TOP_START,
                    &mut demo.state,
                    |ui| {
                        ui.label("Langue");
                        combo = egui::ComboBox::from_id_salt("demo_combo")
                            .selected_text(format!("choix {}", demo.choice))
                            .show_ui(ui, |ui| {
                                for n in 0..4 {
                                    let r = ui.selectable_value(
                                        &mut demo.choice,
                                        n,
                                        format!("choix {n}"),
                                    );
                                    rows.push(r.rect);
                                }
                            })
                            .response
                            .rect;
                        ui.min_rect()
                    },
                );
                demo.combo = combo;
                demo.panel = panel;
                *items = rows;
            });
            out.textures_delta.clear();
        };
        let click = |pos: egui::Pos2| {
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                },
            ]
        };
        // Quelques images après chaque clic (animations, mise en page).
        macro_rules! settle {
            () => {
                for _ in 0..3 {
                    frame(vec![], &mut demo, &mut items);
                }
            };
        }
        settle!();
        let at = demo.button.center();
        frame(click(at), &mut demo, &mut items);
        settle!();
        assert!(
            demo.state.open && demo.panel.is_some(),
            "le bouton ouvre le panneau"
        );

        let at = demo.combo.center();
        frame(click(at), &mut demo, &mut items);
        settle!();
        assert!(demo.state.open, "ouvrir la liste garde le panneau ouvert");
        assert_eq!(items.len(), 4, "la liste déroulante est ouverte");

        let at = items[2].center();
        frame(click(at), &mut demo, &mut items);
        settle!();
        assert_eq!(demo.choice, 2, "le choix est pris en compte");
        assert!(demo.state.open, "choisir garde le panneau ouvert");

        let panel = demo.panel.expect("panneau affiché");
        let outside = egui::pos2(790.0, 590.0);
        assert!(!panel.contains(outside));
        frame(click(outside), &mut demo, &mut items);
        settle!();
        assert!(!demo.state.open, "un clic à l'extérieur ferme le panneau");

        let at = demo.button.center();
        frame(click(at), &mut demo, &mut items);
        frame(click(at), &mut demo, &mut items);
        settle!();
        assert!(!demo.state.open, "le bouton referme le panneau");
    }

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

    /// Contexte egui avec les polices de l'app (police système, arabe…), sur un Mac.
    fn mac_ctx() -> Option<egui::Context> {
        if !std::path::Path::new("/System/Library/Fonts/SFArabic.ttf").exists() {
            return None;
        }
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx, crate::i18n::Lang::Ar);
        let mut frame = ctx.run_ui(Default::default(), |_| {});
        frame.textures_delta.clear();
        Some(ctx)
    }

    /// Caractères d'une ligne dessinée par egui, lus de droite à gauche.
    fn read_rtl(ctx: &egui::Context, line: &str) -> String {
        let job = bidi_job_with_base(line, 12.0, Color32::WHITE, true);
        let galley = ctx.fonts_mut(|f| f.layout_job(job));
        let mut glyphs: Vec<(char, f32)> = galley
            .rows
            .iter()
            .flat_map(|r| r.glyphs.iter())
            .filter(|g| g.advance_width > 0.01)
            .map(|g| (g.chr, g.pos.x))
            .collect();
        glyphs.sort_by(|a, b| b.1.total_cmp(&a.1));
        glyphs.iter().map(|g| g.0).collect()
    }

    /// Avec les vraies polices de l'app (sur un Mac) : chaque traduction arabe et des
    /// textes mêlant arabe, chiffres et latin, coupés en lignes puis dessinés par egui,
    /// se lisent de droite à gauche dans l'ordre. On compare les lettres arabes (une
    /// ligature comme لا ne donne qu'un glyphe pour deux lettres : sous-suite ordonnée).
    #[test]
    fn arabic_text_reads_right_to_left_in_order() {
        let Some(ctx) = mac_ctx() else { return };
        let width = |s: &str| {
            let job = bidi_job_with_base(s, 12.0, Color32::WHITE, true);
            ctx.fonts_mut(|f| f.layout_job(job)).size().x
        };
        let letters = |s: &str| -> String {
            s.chars()
                .filter(|c| matches!(*c as u32, 0x0621..=0x064A))
                .collect()
        };
        let ar: std::collections::HashMap<String, String> =
            serde_json::from_str(include_str!("../../locales/ar.json")).unwrap();
        let mut texts: Vec<(String, String)> = ar.into_iter().collect();
        for sample in [
            "يلصق العنصر المختار مباشرة في التطبيق الذي فتحت منه السجل.",
            "رقم الطلب 12345 — Order ID: AB-77",
            "مرحبا بكم في الحافظة",
        ] {
            texts.push(("exemple".into(), sample.into()));
        }
        let mut multi_line = 0;
        for (key, text) in &texts {
            let lines = break_lines(text, 200.0, width);
            multi_line += usize::from(lines.len() > 1);
            let read: String = lines.iter().map(|l| read_rtl(&ctx, l)).collect();
            let (got, want) = (letters(&read), letters(text));
            let mut expected = want.chars();
            let in_order = got.chars().all(|c| expected.any(|e| e == c));
            assert!(
                in_order && got.chars().count() * 10 >= want.chars().count() * 7,
                "{key} : « {text} » mal ordonné (lu « {got} », lignes {lines:?})"
            );
        }
        assert!(
            multi_line >= 5,
            "des textes sur plusieurs lignes sont testés"
        );
        // Le latin garde son sens : « CopyQ » se lit de gauche à droite.
        let job = bidi_job_with_base("هل ما زال CopyQ يعمل؟", 12.0, Color32::WHITE, true);
        let galley = ctx.fonts_mut(|f| f.layout_job(job));
        let x = |c: char| {
            galley
                .rows
                .iter()
                .flat_map(|r| r.glyphs.iter())
                .find(|g| g.chr == c)
                .map(|g| g.pos.x)
                .unwrap()
        };
        assert!(x('C') < x('o') && x('o') < x('p') && x('p') < x('Q'));
        // Chemins et dimensions isolés : gardent leur sens de gauche à droite.
        for (text, before, after) in [
            (
                format!("منذ 1 د  ·  {}", i18n::ltr_isolate("/tmp")),
                '/',
                't',
            ),
            (
                format!("منذ 2 د · {} · 20 ك.ب", i18n::ltr_isolate("1280 × 720")),
                '1',
                '7',
            ),
        ] {
            let job = bidi_job_with_base(&text, 12.0, Color32::WHITE, true);
            let galley = ctx.fonts_mut(|f| f.layout_job(job));
            let x = |c: char| {
                galley
                    .rows
                    .iter()
                    .flat_map(|r| r.glyphs.iter())
                    .find(|g| g.chr == c)
                    .map(|g| g.pos.x)
                    .unwrap()
            };
            assert!(
                x(before) < x(after),
                "{text:?} : « {before} » doit précéder « {after} »"
            );
        }
        // Ponctuation de fin de phrase à gauche (fin de la lecture).
        let read = read_rtl(&ctx, "عند تجاوزه، يحذف الأقدم.");
        assert!(read.ends_with('.'), "{read}");
    }

    /// Chaque langue, avec les polices que l'app charge pour elle (sur un Mac) :
    /// aucun caractère sans glyphe, lecture de gauche à droite dans l'ordre pour les
    /// langues qui s'écrivent ainsi, et pas de ligne qui commence par une ponctuation
    /// fermante (règle japonaise et chinoise : 、。）」…).
    #[test]
    fn every_language_renders_completely_and_in_order() {
        use crate::i18n::{Lang, ALL};
        if !std::path::Path::new("/System/Library/Fonts/SFNS.ttf").exists() {
            return;
        }
        let sources = [
            (Lang::En, include_str!("../../locales/en.json")),
            (Lang::Fr, include_str!("../../locales/fr.json")),
            (Lang::De, include_str!("../../locales/de.json")),
            (Lang::Es, include_str!("../../locales/es.json")),
            (Lang::It, include_str!("../../locales/it.json")),
            (Lang::PtBr, include_str!("../../locales/pt-BR.json")),
            (Lang::Ja, include_str!("../../locales/ja.json")),
            (Lang::Ko, include_str!("../../locales/ko.json")),
            (Lang::ZhHans, include_str!("../../locales/zh-Hans.json")),
            (Lang::ZhHant, include_str!("../../locales/zh-Hant.json")),
            (Lang::Ar, include_str!("../../locales/ar.json")),
        ];
        assert_eq!(sources.len(), ALL.len());
        let closing = "、。，．,.）」』】〕〉》！？：；!?:;)";
        let mut problems = Vec::new();
        for (lang, source) in sources {
            let ctx = egui::Context::default();
            crate::theme::install_fonts(&ctx, lang);
            let mut frame = ctx.run_ui(Default::default(), |_| {});
            frame.textures_delta.clear();
            let texts: std::collections::BTreeMap<String, String> =
                serde_json::from_str(source).unwrap();
            for (key, text) in &texts {
                let family = if i18n::has_rtl(text) {
                    FontFamily::Name(RTL_FAMILY.into())
                } else {
                    FontFamily::Proportional
                };
                let font = FontId::new(12.0, family);
                let missing: String = text
                    .chars()
                    .filter(|c| !c.is_control())
                    .filter(|c| !ctx.fonts_mut(|f| f.has_glyph(&font, *c)))
                    .collect();
                if !missing.is_empty() {
                    problems.push(format!("{} {key} : sans glyphe {missing:?}", lang.code()));
                }
                if lang.is_rtl() {
                    continue;
                }
                // Gauche à droite : position croissante dans l'ordre du texte, à
                // plusieurs largeurs (le panneau Réglages, les infos-bulles…).
                for width in [160.0, 220.0, 280.0, 340.0] {
                    let galley = ctx.fonts_mut(|f| {
                        f.layout(
                            text.clone(),
                            FontId::proportional(11.5),
                            Color32::WHITE,
                            width,
                        )
                    });
                    for row in galley.rows.iter() {
                        let xs: Vec<f32> = row
                            .glyphs
                            .iter()
                            .filter(|g| g.advance_width > 0.01)
                            .map(|g| g.pos.x)
                            .collect();
                        if xs.windows(2).any(|w| w[1] + 0.5 < w[0]) {
                            problems
                                .push(format!("{} {key} : ordre gauche-droite rompu", lang.code()));
                        }
                        let first = row
                            .glyphs
                            .iter()
                            .map(|g| g.chr)
                            .find(|c| !c.is_whitespace());
                        if galley.rows.len() > 1
                            && first.is_some_and(|c| closing.contains(c))
                            && matches!(lang, Lang::Ja | Lang::ZhHans | Lang::ZhHant | Lang::Ko)
                        {
                            let line: String = row.glyphs.iter().map(|g| g.chr).collect();
                            problems.push(format!(
                                "{} {key} : ligne qui commence par « {} » : {line}",
                                lang.code(),
                                first.unwrap()
                            ));
                        }
                    }
                }
            }
        }
        assert!(
            problems.is_empty(),
            "{} problème(s) :\n{}",
            problems.len(),
            problems.join("\n")
        );
    }
}
