//! Historique du presse-papiers : stockage, recherche et persistance JSON.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Nombre maximal d'éléments conservés.
pub const MAX_ITEMS: usize = 500;
/// Taille maximale d'un élément (au-delà, il est ignoré).
pub const MAX_ITEM_BYTES: usize = 1_000_000;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub text: String,
    /// Horodatage Unix (secondes) de la dernière copie.
    pub copied_at: u64,
}

#[derive(Debug, Default)]
pub struct History {
    items: Vec<Item>,
    path: Option<PathBuf>,
    /// Incrémenté à chaque modification (permet à l'interface de mettre en cache).
    version: u64,
}

#[derive(Serialize, Deserialize)]
struct OnDisk {
    version: u32,
    items: Vec<Item>,
}

impl History {
    /// Charge l'historique depuis `path` (vide si le fichier n'existe pas ou est illisible).
    pub fn load(path: PathBuf) -> Self {
        let items = fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<OnDisk>(&s).ok())
            .map(|d| d.items)
            .unwrap_or_default();
        Self {
            items,
            path: Some(path),
            version: 0,
        }
    }

    pub fn items(&self) -> &[Item] {
        &self.items
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Ajoute un texte en tête. Un doublon est remonté en tête au lieu d'être dupliqué.
    /// Renvoie `true` si l'historique a changé.
    pub fn push(&mut self, text: String) -> bool {
        if text.trim().is_empty() || text.len() > MAX_ITEM_BYTES {
            return false;
        }
        match self.items.iter().position(|it| it.text == text) {
            Some(0) => return false,
            Some(pos) => {
                let mut item = self.items.remove(pos);
                item.copied_at = now_secs();
                self.items.insert(0, item);
            }
            None => {
                self.items.insert(
                    0,
                    Item {
                        text,
                        copied_at: now_secs(),
                    },
                );
                self.items.truncate(MAX_ITEMS);
            }
        }
        self.version += 1;
        true
    }

    pub fn remove(&mut self, index: usize) -> Option<Item> {
        let removed = (index < self.items.len()).then(|| self.items.remove(index));
        if removed.is_some() {
            self.version += 1;
        }
        removed
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.version += 1;
    }

    /// Indices des éléments correspondant à la requête : chaque mot doit apparaître
    /// (insensible à la casse). Une requête vide renvoie tout.
    pub fn search(&self, query: &str) -> Vec<usize> {
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        self.items
            .iter()
            .enumerate()
            .filter(|(_, it)| {
                if words.is_empty() {
                    return true;
                }
                let hay = it.text.to_lowercase();
                words.iter().all(|w| hay.contains(w.as_str()))
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Écrit l'historique sur disque (écriture atomique via fichier temporaire).
    pub fn save(&self) -> io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let data = OnDisk {
            version: 1,
            items: self.items.clone(),
        };
        let json = serde_json::to_string(&data).map_err(io::Error::other)?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, json)?;
        fs::rename(&tmp, path)
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Durée relative lisible en français (« à l'instant », « il y a 5 min »…).
pub fn relative_time(then: u64, now: u64) -> String {
    let d = now.saturating_sub(then);
    match d {
        0..=59 => "à l'instant".to_string(),
        60..=3_599 => format!("il y a {} min", d / 60),
        3_600..=86_399 => format!("il y a {} h", d / 3_600),
        _ => format!("il y a {} j", d / 86_400),
    }
}

/// Aperçu sur une ligne : première ligne, tronquée.
pub fn preview(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    let first = trimmed.lines().next().unwrap_or("").trim();
    let mut out: String = first.chars().take(max_chars).collect();
    if first.chars().count() > max_chars {
        out.push('…');
    }
    out
}

/// Emplacement par défaut : ~/Library/Application Support/com.amine.copyq-rs/history.json sur macOS.
pub fn default_path() -> PathBuf {
    directories::ProjectDirs::from("com", "amine", "copyq-rs")
        .map(|d| d.data_dir().join("history.json"))
        .unwrap_or_else(|| PathBuf::from("history.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h() -> History {
        History::default()
    }

    #[test]
    fn push_adds_to_front() {
        let mut h = h();
        assert!(h.push("a".into()));
        assert!(h.push("b".into()));
        assert_eq!(h.items()[0].text, "b");
        assert_eq!(h.len(), 2);
    }

    #[test]
    fn duplicate_moves_to_front() {
        let mut h = h();
        h.push("a".into());
        h.push("b".into());
        assert!(h.push("a".into()));
        assert_eq!(h.len(), 2);
        assert_eq!(h.items()[0].text, "a");
        assert!(!h.push("a".into()), "déjà en tête : aucun changement");
    }

    #[test]
    fn ignores_blank_and_huge() {
        let mut h = h();
        assert!(!h.push("   \n".into()));
        assert!(!h.push("x".repeat(MAX_ITEM_BYTES + 1)));
        assert_eq!(h.len(), 0);
    }

    #[test]
    fn caps_at_max() {
        let mut h = h();
        for i in 0..MAX_ITEMS + 10 {
            h.push(i.to_string());
        }
        assert_eq!(h.len(), MAX_ITEMS);
        assert_eq!(h.items()[0].text, (MAX_ITEMS + 9).to_string());
    }

    #[test]
    fn search_all_words_case_insensitive() {
        let mut h = h();
        h.push("Hello World".into());
        h.push("hello rust".into());
        h.push("autre".into());
        assert_eq!(h.search("").len(), 3);
        assert_eq!(h.search("HELLO").len(), 2);
        assert_eq!(h.search("world hello"), vec![2]); // ordre : autre, hello rust, Hello World
        assert!(h.search("absent").is_empty());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = std::env::temp_dir().join(format!("copyq-rs-test-{}", std::process::id()));
        let path = dir.join("history.json");
        let mut h = History::load(path.clone());
        h.push("un".into());
        h.push("deux".into());
        h.save().unwrap();
        let loaded = History::load(path);
        assert_eq!(loaded.items(), h.items());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn preview_and_time() {
        assert_eq!(preview("  abc\ndef\n", 10), "abc");
        assert_eq!(preview("abcdef", 3), "abc…");
        assert_eq!(relative_time(100, 130), "à l'instant");
        assert_eq!(relative_time(0, 600), "il y a 10 min");
        assert_eq!(relative_time(0, 7_200), "il y a 2 h");
    }
}
