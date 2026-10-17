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
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            save_passwords: false,
            detect_passwords: true,
            hide_after_copy: true,
            save_images: true,
            auto_paste: true,
        }
    }
}

impl Settings {
    /// Lit les réglages ; valeurs par défaut si le fichier est absent ou illisible.
    pub fn load(path: &Path) -> Self {
        match fs::read_to_string(path) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_else(|e| {
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
        assert!(!s.save_passwords, "les mots de passe doivent être ignorés par défaut");
        assert!(s.hide_after_copy);
        assert!(s.detect_passwords);
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
        };
        s.save(&path);
        assert_eq!(Settings::load(&path), s);

        // Fichier d'une ancienne version, sans tous les champs.
        fs::write(&path, r#"{"save_passwords": true}"#).unwrap();
        let partial = Settings::load(&path);
        assert!(partial.save_passwords);
        assert!(partial.hide_after_copy, "champ absent → valeur par défaut");

        fs::write(&path, "pas du json").unwrap();
        assert_eq!(Settings::load(&path), Settings::default(), "fichier corrompu");
        let _ = fs::remove_dir_all(dir);
    }
}
