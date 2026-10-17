//! Historique du presse-papiers en mémoire : ajout, recherche, suppression.
//! Chaque modification est répercutée en base via `storage::Store`.

use crate::storage::Store;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Nombre maximal d'éléments conservés.
pub const MAX_ITEMS: usize = 500;
/// Taille maximale d'un élément (au-delà, il est ignoré).
pub const MAX_ITEM_BYTES: usize = 1_000_000;
/// Nombre maximal d'images conservées (les plus anciennes sont supprimées).
pub const MAX_IMAGES: usize = 50;

/// Nature d'un élément.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemKind {
    #[default]
    Text,
    /// `text` = empreinte de l'image (fichier `images/<empreinte>.png`).
    Image,
    /// `text` = chemins des fichiers copiés, un par ligne.
    Files,
}

impl ItemKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ItemKind::Text => "text",
            ItemKind::Image => "image",
            ItemKind::Files => "files",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "image" => ItemKind::Image,
            "files" => ItemKind::Files,
            _ => ItemKind::Text,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub text: String,
    /// Horodatage Unix (secondes) de la dernière copie.
    pub copied_at: u64,
    /// Copie marquée confidentielle (mot de passe) : affichée masquée,
    /// exclue de la recherche.
    #[serde(default)]
    pub sensitive: bool,
    #[serde(default)]
    pub kind: ItemKind,
    /// Description affichée (ex. « 1280 × 720 · 245 Ko » pour une image).
    #[serde(default)]
    pub meta: String,
}

impl Item {
    pub fn text(text: impl Into<String>, copied_at: u64, sensitive: bool) -> Self {
        Item { text: text.into(), copied_at, sensitive, ..Default::default() }
    }

    /// Chemins d'un élément « fichiers ».
    pub fn paths(&self) -> Vec<PathBuf> {
        self.text.lines().filter(|l| !l.is_empty()).map(PathBuf::from).collect()
    }
}

#[derive(Default)]
pub struct History {
    items: Vec<Item>,
    /// Absent dans les tests unitaires : historique purement en mémoire.
    store: Option<Store>,
    /// Incrémenté à chaque modification (permet à l'interface de mettre en cache).
    version: u64,
}

/// Ancien format (v0.1) : un fichier JSON, importé une fois en base.
#[derive(Serialize, Deserialize)]
struct OnDisk {
    version: u32,
    items: Vec<Item>,
}

impl History {
    /// Historique adossé à une base (éléments déjà chargés, du plus récent au plus ancien).
    pub fn with_store(store: Store, items: Vec<Item>) -> Self {
        Self {
            items,
            store: Some(store),
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

    /// Ajoute un texte ordinaire en tête (voir `push_with`).
    pub fn push(&mut self, text: String) -> bool {
        self.push_with(text, false)
    }

    /// Ajoute un texte en tête. Un doublon est remonté en tête au lieu d'être dupliqué
    /// (et reste masqué s'il l'était). Renvoie `true` si l'historique a changé.
    pub fn push_with(&mut self, text: String, sensitive: bool) -> bool {
        self.push_item(Item::text(text, now_secs(), sensitive))
    }

    /// Ajoute un élément (texte, image, fichiers) en tête ; un doublon (même nature,
    /// même contenu) est remonté au lieu d'être dupliqué.
    pub fn push_item(&mut self, new: Item) -> bool {
        if new.text.trim().is_empty() || new.text.len() > MAX_ITEM_BYTES {
            return false;
        }
        let same = |it: &Item| it.kind == new.kind && it.text == new.text;
        match self.items.iter().position(same) {
            Some(0) if !new.sensitive || self.items[0].sensitive => return false,
            Some(pos) => {
                let mut item = self.items.remove(pos);
                item.copied_at = now_secs();
                item.sensitive |= new.sensitive;
                if !new.meta.is_empty() {
                    item.meta = new.meta;
                }
                self.items.insert(0, item);
            }
            None => {
                let mut item = new;
                item.copied_at = now_secs();
                self.items.insert(0, item);
                self.evict();
            }
        }
        if let Some(store) = &self.store {
            store.upsert(&self.items[0]);
        }
        self.version += 1;
        true
    }

    /// Applique les limites : `MAX_ITEMS` éléments, dont au plus `MAX_IMAGES` images.
    fn evict(&mut self) {
        let mut evicted = Vec::new();
        let mut images = 0;
        let mut kept = Vec::with_capacity(self.items.len());
        for item in std::mem::take(&mut self.items) {
            let is_image = item.kind == ItemKind::Image;
            if kept.len() >= MAX_ITEMS || (is_image && images >= MAX_IMAGES) {
                evicted.push(item);
                continue;
            }
            images += usize::from(is_image);
            kept.push(item);
        }
        self.items = kept;
        if let Some(store) = &self.store {
            for item in &evicted {
                store.delete(item);
            }
        }
    }

    pub fn remove(&mut self, index: usize) -> Option<Item> {
        let removed = (index < self.items.len()).then(|| self.items.remove(index));
        if let Some(item) = &removed {
            if let Some(store) = &self.store {
                store.delete(item);
            }
            self.version += 1;
        }
        removed
    }

    /// Marque (ou démarque) un élément comme mot de passe : il est alors chiffré
    /// en base, masqué à l'écran et exclu de la recherche.
    pub fn set_sensitive(&mut self, index: usize, sensitive: bool) -> bool {
        let Some(old) = self.items.get(index).cloned() else {
            return false;
        };
        if old.sensitive == sensitive {
            return false;
        }
        self.items[index].sensitive = sensitive;
        if let Some(store) = &self.store {
            store.delete(&old);
            store.upsert(&self.items[index]);
        }
        self.version += 1;
        true
    }

    /// Applique la détection aux éléments déjà enregistrés (ex. un mot de passe
    /// Dashlane copié avant cette version) : chiffrés et masqués si `keep`,
    /// supprimés sinon. Renvoie le nombre d'éléments concernés.
    pub fn reclassify_probable_passwords(&mut self, keep: bool) -> usize {
        let targets: Vec<usize> = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, it)| {
                it.kind == ItemKind::Text
                    && !it.sensitive
                    && crate::pasteboard::looks_like_password(&it.text)
            })
            .map(|(i, _)| i)
            .collect();
        for &i in targets.iter().rev() {
            if keep {
                self.set_sensitive(i, true);
            } else {
                self.remove(i);
            }
        }
        targets.len()
    }

    pub fn sensitive_count(&self) -> usize {
        self.items.iter().filter(|it| it.sensitive).count()
    }

    /// Supprime tous les mots de passe enregistrés ; renvoie leur nombre.
    pub fn remove_sensitive(&mut self) -> usize {
        let before = self.items.len();
        let store = &self.store;
        self.items.retain(|it| {
            if it.sensitive {
                if let Some(store) = store {
                    store.delete(it);
                }
            }
            !it.sensitive
        });
        let removed = before - self.items.len();
        if removed > 0 {
            self.version += 1;
        }
        removed
    }

    pub fn clear(&mut self) {
        self.items.clear();
        if let Some(store) = &self.store {
            store.clear();
        }
        self.version += 1;
    }

    /// Indices des éléments correspondant à la requête : chaque mot doit apparaître
    /// (insensible à la casse). Une requête vide renvoie tout. Les éléments masqués
    /// (mots de passe) ne sont jamais retrouvés par la recherche.
    pub fn search(&self, query: &str) -> Vec<usize> {
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        self.items
            .iter()
            .enumerate()
            .filter(|(_, it)| {
                if words.is_empty() {
                    return true;
                }
                if it.sensitive {
                    return false;
                }
                let hay = match it.kind {
                    // Une image se retrouve en cherchant « image » ou ses dimensions.
                    ItemKind::Image => format!("image {}", it.meta).to_lowercase(),
                    _ => it.text.to_lowercase(),
                };
                words.iter().all(|w| hay.contains(w.as_str()))
            })
            .map(|(i, _)| i)
            .collect()
    }
}

/// Lit l'ancien fichier `history.json` (None s'il est absent ou illisible).
pub fn read_legacy_json(path: &Path) -> Option<Vec<Item>> {
    let s = fs::read_to_string(path).ok()?;
    serde_json::from_str::<OnDisk>(&s).ok().map(|d| d.items)
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

/// Dossier des données : ~/Library/Application Support/com.amine.copyq-rs sur macOS.
pub fn data_dir() -> PathBuf {
    directories::ProjectDirs::from("com", "amine", "copyq-rs")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Ancien fichier JSON de la v0.1, importé automatiquement s'il existe.
pub fn legacy_json_path() -> PathBuf {
    data_dir().join("history.json")
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
    fn sensitive_items_are_kept_flagged_and_hidden_from_search() {
        let mut h = h();
        assert!(h.push_with("motdepasse123".into(), true));
        h.push("texte ordinaire".into());
        assert!(h.items()[1].sensitive);
        assert!(h.search("motdepasse").is_empty(), "jamais trouvé par la recherche");
        assert_eq!(h.search("").len(), 2, "mais bien listé");
        // Recopié comme texte ordinaire : reste masqué.
        h.push("motdepasse123".into());
        assert!(h.items()[0].sensitive);
        // Un texte ordinaire déjà en tête, recopié depuis un gestionnaire : devient masqué.
        h.push("partagé".into());
        assert!(h.push_with("partagé".into(), true));
        assert!(h.items()[0].sensitive);

        assert_eq!(h.sensitive_count(), 2);
        // Démarquer puis remarquer manuellement.
        assert!(h.set_sensitive(0, false));
        assert!(!h.items()[0].sensitive);
        assert!(!h.set_sensitive(0, false), "déjà démarqué");
        assert!(h.set_sensitive(0, true));
        assert_eq!(h.remove_sensitive(), 2);
        assert_eq!(h.sensitive_count(), 0);
        assert_eq!(h.len(), 1);
    }

    #[test]
    fn reclassifies_probable_passwords_already_stored() {
        let mut h = h();
        h.push("Xk9#mL2$pQ8!vR".into()); // copié depuis une extension, non marqué
        h.push("texte ordinaire".into());
        let mut kept = h_clone(&h);
        assert_eq!(kept.reclassify_probable_passwords(true), 1);
        assert_eq!(kept.len(), 2);
        assert!(kept.items()[1].sensitive);
        assert_eq!(h.reclassify_probable_passwords(false), 1);
        assert_eq!(h.len(), 1);
        assert_eq!(h.items()[0].text, "texte ordinaire");
    }

    fn h_clone(src: &History) -> History {
        let mut h = History::default();
        for it in src.items().iter().rev() {
            h.push_with(it.text.clone(), it.sensitive);
        }
        h
    }

    #[test]
    fn images_and_files_dedup_by_kind_and_are_capped() {
        let mut h = h();
        let img = |hash: &str| Item { text: hash.into(), kind: ItemKind::Image, meta: "1 × 1 · 1 o".into(), ..Default::default() };
        assert!(h.push_item(img("aaa")));
        assert!(h.push_item(Item { text: "/tmp/a.mov".into(), kind: ItemKind::Files, ..Default::default() }));
        h.push("aaa".into()); // même contenu mais texte : élément distinct
        assert_eq!(h.len(), 3);
        assert!(h.push_item(img("aaa"))); // doublon : remonte
        assert_eq!(h.len(), 3);
        assert_eq!(h.items()[0].kind, ItemKind::Image);
        assert_eq!(h.search("image").len(), 1);
        assert_eq!(h.search("a.mov").len(), 1);

        for i in 0..MAX_IMAGES + 5 {
            h.push_item(img(&format!("h{i}")));
        }
        let images = h.items().iter().filter(|i| i.kind == ItemKind::Image).count();
        assert_eq!(images, MAX_IMAGES, "au plus {MAX_IMAGES} images");
        assert!(h.items().iter().any(|i| i.kind == ItemKind::Files), "le reste est conservé");
        assert_eq!(h.items()[0].text, format!("h{}", MAX_IMAGES + 4));
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
    fn history_changes_reach_the_database() {
        use std::time::Duration;
        let dir = std::env::temp_dir().join(format!("copyq-rs-hist-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let db = dir.join("h.db");
        let key = crate::crypto::generate_key();
        {
            let (store, items) = Store::open(db.clone(), None, None, Box::new(move || Ok(key))).unwrap();
            let mut h = History::with_store(store.clone(), items);
            h.push("un".into());
            h.push("deux".into());
            h.push("trois".into());
            h.push("un".into()); // remonte en tête
            h.remove(1); // supprime « trois »
            assert!(store.close(Duration::from_secs(5)));
        }
        let (store, items) = Store::open(db, None, None, Box::new(move || Ok(key))).unwrap();
        let texts: Vec<_> = items.iter().map(|i| i.text.as_str()).collect();
        assert_eq!(texts, vec!["un", "deux"]);
        store.close(Duration::from_secs(5));
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
