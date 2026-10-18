//! Traductions de l'interface : 10 langues, choisies parmi les principaux marchés de
//! l'App Store (États-Unis/Royaume-Uni, Japon, Allemagne, Corée, France, Brésil,
//! Espagne/Amérique latine, Chine, Taïwan/Hong Kong, Italie).
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
}

pub const ALL: [Lang; 10] = [
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
        }
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

fn tables() -> &'static [Table] {
    static TABLES: OnceLock<Vec<Table>> = OnceLock::new();
    TABLES.get_or_init(|| ALL.iter().map(|l| parse(l.source())).collect())
}

/// Lit un fichier de langue ; les textes vivent jusqu'à la fin du programme.
fn parse(source: &'static str) -> Table {
    let map: HashMap<String, String> = serde_json::from_str(source).unwrap_or_else(|e| {
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

/// Texte d'une clé dans une langue donnée (secours : anglais, puis la clé elle-même).
pub fn tr_in(lang: Lang, key: &'static str) -> &'static str {
    let t = tables();
    t[lang.index()]
        .get(key)
        .or_else(|| t[Lang::En.index()].get(key))
        .copied()
        .unwrap_or(key)
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
            let pattern = "t!(\"";
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
        assert!(used > 50, "{used} clés utilisées");
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
    }
}
