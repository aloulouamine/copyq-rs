//! Réglages de l'utilisateur, conservés entre les lancements
//! dans `settings.json` (dossier de données de l'app).

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Enregistrer aussi les copies marquées confidentielles (mots de passe).
    /// Désactivé par défaut : c'est le choix le plus sûr.
    pub save_passwords: bool,
    /// Traiter comme mots de passe les textes qui en ont l'apparence (copies
    /// d'extensions de navigateur, qui ne sont pas marquées confidentielles).
    pub detect_passwords: bool,
    /// Masquer la fenêtre après avoir choisi un élément.
    pub hide_after_copy: bool,
    /// Enregistrer les images copiées (captures d'écran…).
    pub save_images: bool,
    /// Coller l'élément choisi dans l'application d'où l'historique a été ouvert.
    pub auto_paste: bool,
    /// Limites (valeurs par défaut = celles d'origine, en dur dans le code).
    pub max_items: usize,
    pub max_images: usize,
    pub max_text_kb: usize,
    /// Taille cumulée maximale des textes en mémoire (Mo).
    pub max_total_mb: usize,
    pub max_image_mp: u32,
    /// 0 = conservation illimitée.
    pub retention_days: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            save_passwords: false,
            detect_passwords: true,
            hide_after_copy: true,
            save_images: true,
            auto_paste: true,
            max_items: crate::history::MAX_ITEMS,
            max_images: crate::history::MAX_IMAGES,
            max_text_kb: crate::history::MAX_ITEM_BYTES / 1000,
            max_total_mb: crate::history::MAX_TOTAL_BYTES / 1_000_000,
            max_image_mp: (crate::media::MAX_PIXELS / 1_000_000) as u32,
            retention_days: 0,
        }
    }
}

/// Bornes proposées dans l'interface (et appliquées à la lecture du fichier).
pub const ITEMS_RANGE: std::ops::RangeInclusive<usize> = 10..=10_000;
pub const IMAGES_RANGE: std::ops::RangeInclusive<usize> = 0..=500;
pub const TEXT_KB_RANGE: std::ops::RangeInclusive<usize> = 1..=10_000;
pub const TOTAL_MB_RANGE: std::ops::RangeInclusive<usize> = 10..=2_000;
pub const IMAGE_MP_RANGE: std::ops::RangeInclusive<u32> = 1..=200;
pub const RETENTION_CHOICES: [(u32, &str); 6] = [
    (0, "Illimitée"),
    (1, "1 jour"),
    (7, "7 jours"),
    (30, "30 jours"),
    (90, "90 jours"),
    (365, "1 an"),
];

impl Settings {
    /// Limites de l'historique correspondant à ces réglages.
    pub fn limits(&self) -> crate::history::Limits {
        crate::history::Limits {
            max_items: self.max_items,
            max_images: self.max_images,
            max_text_bytes: self.max_text_kb * 1000,
            max_total_bytes: self.max_total_mb * 1_000_000,
            retention_days: self.retention_days,
        }
    }

    pub fn max_image_pixels(&self) -> u64 {
        u64::from(self.max_image_mp) * 1_000_000
    }

    /// Ramène les valeurs dans les bornes (fichier modifié à la main…).
    pub fn clamp(&mut self) {
        let c = |v: usize, r: &std::ops::RangeInclusive<usize>| v.clamp(*r.start(), *r.end());
        self.max_items = c(self.max_items, &ITEMS_RANGE);
        self.max_images = c(self.max_images, &IMAGES_RANGE);
        self.max_text_kb = c(self.max_text_kb, &TEXT_KB_RANGE);
        self.max_total_mb = c(self.max_total_mb, &TOTAL_MB_RANGE);
        self.max_image_mp = self
            .max_image_mp
            .clamp(*IMAGE_MP_RANGE.start(), *IMAGE_MP_RANGE.end());
    }

    /// Les limites (seulement) remises aux valeurs par défaut.
    pub fn with_default_limits(&self) -> Self {
        let d = Self::default();
        Self {
            max_items: d.max_items,
            max_images: d.max_images,
            max_text_kb: d.max_text_kb,
            max_total_mb: d.max_total_mb,
            max_image_mp: d.max_image_mp,
            retention_days: d.retention_days,
            ..self.clone()
        }
    }
}

impl Settings {
    /// Lit les réglages ; valeurs par défaut si le fichier est absent ou illisible.
    pub fn load(path: &Path) -> Self {
        match fs::read_to_string(path) {
            Ok(s) => serde_json::from_str::<Self>(&s)
                .map(|mut st| {
                    st.clamp();
                    st
                })
                .unwrap_or_else(|e| {
                    log::warn!("réglages illisibles ({e}) : valeurs par défaut");
                    Self::default()
                }),
            Err(_) => Self::default(),
        }
    }

    /// Écrit les réglages (écriture atomique via fichier temporaire).
    pub fn save(&self, path: &Path) {
        let result = (|| -> std::io::Result<()> {
            if let Some(dir) = path.parent() {
                fs::create_dir_all(dir)?;
            }
            let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
            let tmp = path.with_extension("json.tmp");
            fs::write(&tmp, json)?;
            fs::rename(&tmp, path)
        })();
        if let Err(e) = result {
            log::error!("réglages non enregistrés : {e}");
        }
    }
}

pub fn default_path() -> PathBuf {
    crate::history::data_dir().join("settings.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_safe() {
        let s = Settings::default();
        assert!(
            !s.save_passwords,
            "les mots de passe doivent être ignorés par défaut"
        );
        assert!(s.hide_after_copy);
        assert!(s.detect_passwords);
    }

    #[test]
    fn default_limits_are_the_original_hardcoded_values() {
        let s = Settings::default();
        assert_eq!(s.max_items, 500);
        assert_eq!(s.max_images, 50);
        assert_eq!(s.max_text_kb, 1000);
        assert_eq!(s.max_total_mb, 200);
        assert_eq!(s.max_image_mp, 50);
        assert_eq!(s.retention_days, 0);
        assert_eq!(s.limits(), crate::history::Limits::default());
        assert_eq!(s.max_image_pixels(), crate::media::MAX_PIXELS);
    }

    #[test]
    fn out_of_range_values_are_clamped_and_limits_reset() {
        let dir = std::env::temp_dir().join(format!("copyq-rs-limits-{}", std::process::id()));
        let path = dir.join("settings.json");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            &path,
            r#"{"max_items": 3, "max_image_mp": 9999, "auto_paste": false}"#,
        )
        .unwrap();
        let s = Settings::load(&path);
        assert_eq!(s.max_items, 10);
        assert_eq!(s.max_image_mp, 200);
        let reset = s.with_default_limits();
        assert_eq!(reset.max_items, 500);
        assert!(!reset.auto_paste, "les autres réglages sont conservés");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn roundtrip_and_partial_file() {
        let dir = std::env::temp_dir().join(format!("copyq-rs-settings-{}", std::process::id()));
        let path = dir.join("settings.json");
        assert_eq!(Settings::load(&path), Settings::default(), "fichier absent");

        let s = Settings {
            save_passwords: true,
            detect_passwords: false,
            hide_after_copy: false,
            save_images: false,
            auto_paste: false,
            max_items: 42,
            max_images: 3,
            max_text_kb: 64,
            max_total_mb: 50,
            max_image_mp: 12,
            retention_days: 30,
        };
        s.save(&path);
        assert_eq!(Settings::load(&path), s);

        // Fichier d'une ancienne version, sans tous les champs.
        fs::write(&path, r#"{"save_passwords": true}"#).unwrap();
        let partial = Settings::load(&path);
        assert!(partial.save_passwords);
        assert!(partial.hide_after_copy, "champ absent → valeur par défaut");

        fs::write(&path, "pas du json").unwrap();
        assert_eq!(
            Settings::load(&path),
            Settings::default(),
            "fichier corrompu"
        );
        let _ = fs::remove_dir_all(dir);
    }
}
