//! Traductions de l'interface : 11 langues, choisies parmi les principaux marchés de
//! l'App Store (États-Unis/Royaume-Uni, Japon, Allemagne, Corée, France, Brésil,
//! Espagne/Amérique latine, Chine, Taïwan/Hong Kong, Italie, pays du Golfe).
//!
//! L'arabe s'écrit de droite à gauche : `is_rtl()` inverse alors la mise en page.
//!
//! Les textes sont dans `locales/<langue>.json` (embarqués dans le binaire) ; l'anglais
//! sert de référence et de secours. Dans le code : `t!("clé")`, ou avec des valeurs
//! `t!("clé", n = 3)` qui remplace `{n}`. Des tests vérifient que chaque langue a
//! toutes les clés, les mêmes `{paramètres}`, et que chaque clé utilisée existe.
//!
//! Le journal (`log::…`) reste en français : il s'adresse au développeur.

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    En,
    Fr,
    De,
    Es,
    It,
    PtBr,
    Ja,
    Ko,
    ZhHans,
    ZhHant,
    Ar,
}

pub const ALL: [Lang; 11] = [
    Lang::En,
    Lang::Fr,
    Lang::De,
    Lang::Es,
    Lang::It,
    Lang::PtBr,
    Lang::Ja,
    Lang::Ko,
    Lang::ZhHans,
    Lang::ZhHant,
    Lang::Ar,
];

impl Lang {
    /// Code enregistré dans les réglages (et nom du fichier `locales/<code>.json`).
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Fr => "fr",
            Lang::De => "de",
            Lang::Es => "es",
            Lang::It => "it",
            Lang::PtBr => "pt-BR",
            Lang::Ja => "ja",
            Lang::Ko => "ko",
            Lang::ZhHans => "zh-Hans",
            Lang::ZhHant => "zh-Hant",
            Lang::Ar => "ar",
        }
    }

    /// Nom de la langue dans cette langue (menu de choix).
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Fr => "Français",
            Lang::De => "Deutsch",
            Lang::Es => "Español",
            Lang::It => "Italiano",
            Lang::PtBr => "Português (Brasil)",
            Lang::Ja => "日本語",
            Lang::Ko => "한국어",
            Lang::ZhHans => "简体中文",
            Lang::ZhHant => "繁體中文",
            Lang::Ar => "العربية",
        }
    }

    /// Écriture de droite à gauche : mise en page en miroir.
    pub fn is_rtl(self) -> bool {
        self == Lang::Ar
    }

    /// Écritures chinoise, japonaise ou coréenne : il faut une police système en plus.
    pub fn is_cjk(self) -> bool {
        matches!(self, Lang::Ja | Lang::Ko | Lang::ZhHans | Lang::ZhHant)
    }

    fn source(self) -> &'static str {
        match self {
            Lang::En => include_str!("../locales/en.json"),
            Lang::Fr => include_str!("../locales/fr.json"),
            Lang::De => include_str!("../locales/de.json"),
            Lang::Es => include_str!("../locales/es.json"),
            Lang::It => include_str!("../locales/it.json"),
            Lang::PtBr => include_str!("../locales/pt-BR.json"),
            Lang::Ja => include_str!("../locales/ja.json"),
            Lang::Ko => include_str!("../locales/ko.json"),
            Lang::ZhHans => include_str!("../locales/zh-Hans.json"),
            Lang::ZhHant => include_str!("../locales/zh-Hant.json"),
            Lang::Ar => include_str!("../locales/ar.json"),
        }
    }

    fn index(self) -> usize {
        ALL.iter().position(|l| *l == self).unwrap_or(0)
    }

    /// Code exact, tel qu'enregistré dans les réglages.
    pub fn from_code(code: &str) -> Option<Lang> {
        ALL.into_iter().find(|l| l.code() == code)
    }

    /// Langue d'un identifiant de langue système (BCP 47 : « fr-FR », « zh-Hant-TW »,
    /// « pt-PT », « es-419 »…), ou `None` si elle n'est pas traduite.
    pub fn from_locale(tag: &str) -> Option<Lang> {
        let tag = tag.replace('_', "-");
        let mut parts = tag.split('-');
        let lang = parts.next()?.to_ascii_lowercase();
        let rest: Vec<String> = parts.map(str::to_ascii_lowercase).collect();
        let has = |s: &str| rest.iter().any(|p| p == s);
        Some(match lang.as_str() {
            "en" => Lang::En,
            "fr" => Lang::Fr,
            "de" => Lang::De,
            "es" => Lang::Es,
            "it" => Lang::It,
            "pt" => Lang::PtBr,
            "ja" => Lang::Ja,
            "ko" => Lang::Ko,
            "zh" if has("hant") || has("tw") || has("hk") || has("mo") => Lang::ZhHant,
            "zh" => Lang::ZhHans,
            "ar" => Lang::Ar,
            _ => return None,
        })
    }
}

/// Langue par défaut avant tout réglage : le français (langue d'origine de l'app, et
/// celle attendue par les tests unitaires).
static CURRENT: AtomicUsize = AtomicUsize::new(1);

pub fn current() -> Lang {
    ALL[CURRENT.load(Ordering::Relaxed) % ALL.len()]
}

/// Interface en miroir (langue écrite de droite à gauche) ?
pub fn is_rtl() -> bool {
    current().is_rtl()
}

pub fn set(lang: Lang) {
    CURRENT.store(lang.index(), Ordering::Relaxed);
}

/// Réglage « auto » ou code de langue → langue effective.
pub fn resolve(setting: &str) -> Lang {
    Lang::from_code(setting).unwrap_or_else(detect)
}

/// Première langue préférée de macOS qui est traduite ; anglais sinon.
pub fn detect() -> Lang {
    preferred_languages()
        .iter()
        .find_map(|tag| Lang::from_locale(tag))
        .unwrap_or(Lang::En)
}

#[cfg(target_os = "macos")]
fn preferred_languages() -> Vec<String> {
    objc2::rc::autoreleasepool(|_| {
        objc2_foundation::NSLocale::preferredLanguages()
            .iter()
            .map(|s| s.to_string())
            .collect()
    })
}

#[cfg(not(target_os = "macos"))]
fn preferred_languages() -> Vec<String> {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .map(|v| v.split('.').next().unwrap_or_default().to_string())
        .collect()
}

type Table = HashMap<&'static str, &'static str>;

/// Textes tels qu'écrits (ordre logique).
fn tables() -> &'static [Table] {
    static TABLES: OnceLock<Vec<Table>> = OnceLock::new();
    TABLES.get_or_init(|| ALL.iter().map(|l| parse(*l)).collect())
}

/// Marques de direction (invisibles) : droite à gauche, gauche à droite.
const RLM: char = '\u{200F}';
const LRM: char = '\u{200E}';

/// Lit un fichier de langue ; les textes vivent jusqu'à la fin du programme.
fn parse(lang: Lang) -> Table {
    let map: HashMap<String, String> = serde_json::from_str(lang.source()).unwrap_or_else(|e| {
        log::error!("fichier de langue invalide : {e}");
        HashMap::new()
    });
    map.into_iter()
        .map(|(k, v)| {
            let k: &'static str = Box::leak(k.into_boxed_str());
            let v: &'static str = Box::leak(v.into_boxed_str());
            (k, v)
        })
        .collect()
}

fn lookup(tables: &'static [Table], lang: Lang, key: &'static str) -> &'static str {
    tables[lang.index()]
        .get(key)
        .or_else(|| tables[Lang::En.index()].get(key))
        .copied()
        .unwrap_or(key)
}

/// Texte d'une clé (secours : anglais, puis la clé elle-même).
pub fn tr_in(lang: Lang, key: &'static str) -> &'static str {
    lookup(tables(), lang, key)
}

pub fn tr(key: &'static str) -> &'static str {
    tr_in(current(), key)
}

/// Remplace les `{nom}` du texte par les valeurs données.
pub fn fill(template: &str, args: &[(&str, &dyn Display)]) -> String {
    let mut out = template.to_string();
    for (name, value) in args {
        out = out.replace(&format!("{{{name}}}"), &value.to_string());
    }
    out
}

pub fn trf(key: &'static str, args: &[(&str, &dyn Display)]) -> String {
    fill(tr(key), args)
}

/// Morceau de texte à dessiner : les morceaux sont dans l'ordre visuel, de gauche à
/// droite. `arabic` : écriture de droite à gauche, que egui met en forme et retourne
/// lui-même (à composer avec une police arabe en tête, pour qu'il reste d'un seul tenant).
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub text: String,
    pub arabic: bool,
}

/// Lettres et signes des écritures de droite à gauche (arabe, hébreu…).
pub fn is_rtl_char(c: char) -> bool {
    matches!(c as u32, 0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF)
}

pub fn has_rtl(text: &str) -> bool {
    text.chars().any(is_rtl_char)
}

/// Découpe une ligne en morceaux dans l'ordre visuel (algorithme bidirectionnel
/// d'Unicode).
///
/// egui ne connaît pas le sens d'écriture : il coupe le texte par police, met en forme
/// chaque morceau (harfrust retourne alors un morceau arabe) et pose les morceaux de
/// gauche à droite dans l'ordre reçu. Une phrase arabe dont les espaces ou la
/// ponctuation viennent d'une autre police sortirait donc mot par mot dans le désordre.
/// On lui donne ici les morceaux déjà rangés : les passages de droite à gauche sont
/// découpés en mots arabes (espaces compris) et en signes isolés, dont on inverse
/// l'ordre et dont on retourne les parenthèses et guillemets.
pub fn visual_pieces(text: &str, rtl_base: bool) -> Vec<Piece> {
    use unicode_bidi::{BidiInfo, Level};
    let text = strip_marks(text);
    let base = if rtl_base { Level::rtl() } else { Level::ltr() };
    let info = BidiInfo::new(&text, Some(base));
    let mut out = Vec::new();
    for para in &info.paragraphs {
        let (levels, runs) = info.visual_runs(para, para.range.clone());
        for run in runs {
            let piece = text[run.clone()].trim_end_matches(['\n', '\r']);
            if piece.is_empty() {
                continue;
            }
            if !levels[run.start].is_rtl() {
                out.push(Piece {
                    text: piece.to_string(),
                    arabic: false,
                });
                continue;
            }
            let mut atoms: Vec<Piece> = Vec::new();
            for c in piece.chars() {
                match atoms.last_mut() {
                    Some(last) if last.arabic && (is_rtl_char(c) || c.is_whitespace()) => {
                        last.text.push(c)
                    }
                    _ if is_rtl_char(c) => atoms.push(Piece {
                        text: c.to_string(),
                        arabic: true,
                    }),
                    _ => atoms.push(Piece {
                        text: mirror(c).to_string(),
                        arabic: false,
                    }),
                }
            }
            out.extend(atoms.into_iter().rev());
        }
    }
    out
}

/// Glyphe en miroir dans un passage de droite à gauche (parenthèses, guillemets…).
fn mirror(c: char) -> char {
    match c {
        '(' => ')',
        ')' => '(',
        '[' => ']',
        ']' => '[',
        '{' => '}',
        '}' => '{',
        '<' => '>',
        '>' => '<',
        '«' => '»',
        '»' => '«',
        '‹' => '›',
        '›' => '‹',
        _ => c,
    }
}

/// Vrai si le texte commence (première lettre) par une écriture de droite à gauche.
pub fn starts_rtl(text: &str) -> bool {
    text.chars()
        .find(|c| c.is_alphabetic())
        .is_some_and(is_rtl_char)
}

/// Texte sans les marques de direction (U+200E, U+200F).
pub fn strip_marks(text: &str) -> String {
    text.chars().filter(|c| !matches!(*c, LRM | RLM)).collect()
}

/// Nombre à une décimale avec le séparateur de la langue (« 2,5 » ou « 2.5 »).
pub fn decimal(x: f64) -> String {
    format!("{x:.1}").replace('.', tr("num.decimal"))
}

/// `t!("clé")` → `&'static str` ; `t!("clé", n = 3, error = e)` → `String`.
#[macro_export]
macro_rules! t {
    ($key:literal) => {
        $crate::i18n::tr($key)
    };
    ($key:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::trf(
            $key,
            &[$((stringify!($name), &$value as &dyn ::std::fmt::Display)),+],
        )
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn placeholders(s: &str) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        let mut rest = s;
        while let Some(start) = rest.find('{') {
            let Some(len) = rest[start..].find('}') else {
                break;
            };
            out.insert(rest[start + 1..start + len].to_string());
            rest = &rest[start + len + 1..];
        }
        out
    }

    #[test]
    fn every_language_has_every_key_with_the_same_placeholders() {
        let t = tables();
        let reference = &t[Lang::En.index()];
        assert!(reference.len() > 50, "fichier anglais lu");
        for lang in ALL {
            let table = &t[lang.index()];
            for (key, en) in reference {
                let Some(text) = table.get(key) else {
                    panic!("{} : clé manquante « {key} »", lang.code());
                };
                assert!(!text.is_empty(), "{} : « {key} » vide", lang.code());
                assert_eq!(
                    placeholders(text),
                    placeholders(en),
                    "{} : paramètres différents pour « {key} »",
                    lang.code()
                );
            }
            for key in table.keys() {
                assert!(
                    reference.contains_key(key),
                    "{} : clé en trop « {key} »",
                    lang.code()
                );
            }
        }
    }

    /// Chaque `t!("…")` du code doit exister dans les fichiers de langue.
    #[test]
    fn every_key_used_in_the_code_exists() {
        let en = &tables()[Lang::En.index()];
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = vec![src.clone()];
        let mut used = 0;
        while let Some(path) = files.pop() {
            if path.is_dir() {
                files.extend(
                    std::fs::read_dir(&path)
                        .unwrap()
                        .flatten()
                        .map(|e| e.path()),
                );
                continue;
            }
            if path.extension().is_none_or(|e| e != "rs") || path.ends_with("i18n.rs") {
                continue;
            }
            let code = std::fs::read_to_string(&path).unwrap();
            for pattern in ["t!(\"", "tp!(\""] {
                let mut from = 0;
                while let Some(pos) = code[from..].find(pattern) {
                    let start = from + pos;
                    from = start + pattern.len();
                    // `t!(` seul, pas la fin de `format!(`, `print!(`…
                    let prev = code[..start].chars().next_back();
                    if prev.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                        continue;
                    }
                    let key = code[from..].split('"').next().unwrap();
                    assert!(
                        en.contains_key(key),
                        "{} : clé inconnue « {key} »",
                        path.display()
                    );
                    used += 1;
                }
            }
        }
        assert!(used > 50, "{used} clés utilisées");
    }

    fn texts(pieces: &[Piece]) -> Vec<&str> {
        pieces.iter().map(|p| p.text.as_str()).collect()
    }

    #[test]
    fn right_to_left_lines_are_cut_into_pieces_in_visual_order() {
        // « أ B ج » (droite à gauche) : de gauche à droite, « ج », « B », « أ ».
        let p = visual_pieces("أ B ج", true);
        assert_eq!(texts(&p), vec!["ج", " ", "B", "أ "]);
        assert!(p[0].arabic && !p[2].arabic && p[3].arabic);
        // Ponctuation et symboles : isolés, à leur place, parenthèses retournées.
        let p = visual_pieces("مسح الكل (⌘⇧Z).", true);
        assert_eq!(texts(&p), vec![".", "(", "Z", "⇧", "⌘", ")", "مسح الكل "]);
        // Les chiffres et le latin gardent leur ordre.
        let p = visual_pieces("رقم 12345 — Order ID", true);
        assert_eq!(texts(&p), vec!["Order ID", " ", "—", " ", "12345", "رقم "]);
        // Texte sans arabe : un seul morceau.
        assert_eq!(texts(&visual_pieces("CopyQ 2", false)), vec!["CopyQ 2"]);
        assert!(visual_pieces("", true).is_empty());
        assert!(has_rtl("abc ج") && !has_rtl("abc"));
        assert!(starts_rtl("12 ج") && !starts_rtl("a ج"));
    }

    #[test]
    fn system_locales_are_matched() {
        assert_eq!(Lang::from_locale("fr-FR"), Some(Lang::Fr));
        assert_eq!(Lang::from_locale("en-GB"), Some(Lang::En));
        assert_eq!(Lang::from_locale("pt-PT"), Some(Lang::PtBr));
        assert_eq!(Lang::from_locale("es-419"), Some(Lang::Es));
        assert_eq!(Lang::from_locale("zh-Hans-CN"), Some(Lang::ZhHans));
        assert_eq!(Lang::from_locale("zh-Hant-TW"), Some(Lang::ZhHant));
        assert_eq!(Lang::from_locale("zh-HK"), Some(Lang::ZhHant));
        assert_eq!(Lang::from_locale("ja_JP"), Some(Lang::Ja));
        assert_eq!(Lang::from_locale("ar-SA"), Some(Lang::Ar));
        assert_eq!(Lang::from_locale("ar_AE"), Some(Lang::Ar));
        assert!(Lang::Ar.is_rtl() && !Lang::Fr.is_rtl());
        assert_eq!(Lang::from_locale("nl-NL"), None);
        assert_eq!(resolve("de"), Lang::De);
        for lang in ALL {
            assert_eq!(Lang::from_code(lang.code()), Some(lang));
        }
    }

    #[test]
    fn texts_are_filled_and_fall_back() {
        assert_eq!(tr_in(Lang::En, "row.lines"), "{n} lines");
        assert_eq!(
            fill(tr_in(Lang::Ja, "toast.copy_failed"), &[("error", &"x")]),
            "コピーできませんでした:x"
        );
        assert_eq!(tr_in(Lang::De, "clé.inexistante"), "clé.inexistante");
        assert_eq!(tr_in(Lang::Ar, "num.decimal"), ".");
    }
}
