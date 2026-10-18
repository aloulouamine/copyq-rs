//! Historique du presse-papiers en mémoire : ajout, recherche, suppression.
//! Chaque modification est répercutée en base via `storage::Store`.

use crate::storage::Store;
use serde::{Deserialize, Serialize};
use std::cell::OnceCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Valeurs par défaut des limites (modifiables dans Réglages → Limites).
/// Nombre maximal d'éléments conservés.
pub const MAX_ITEMS: usize = 500;
/// Taille maximale d'un élément (au-delà, il est ignoré).
pub const MAX_ITEM_BYTES: usize = 1_000_000;
/// Nombre maximal d'images conservées (les plus anciennes sont supprimées).
pub const MAX_IMAGES: usize = 50;
/// Taille totale maximale des textes gardés en mémoire (les plus anciens sont supprimés).
pub const MAX_TOTAL_BYTES: usize = 200_000_000;

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
    /// Valeurs dérivées du texte, calculées au premier besoin puis gardées
    /// (jamais enregistrées). À vider si `text`, `kind` ou `meta` change.
    #[serde(skip)]
    pub cache: ItemCache,
}

/// Statistiques d'un texte, affichées sous chaque élément.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStats {
    pub lines: usize,
    pub chars: usize,
    /// Aucun espace ni retour à la ligne (une fois les bords retirés).
    pub single_word: bool,
}

/// Cache paresseux d'un élément (sans effet sur l'égalité ni l'affichage de débogage).
#[derive(Default)]
pub struct ItemCache {
    search: OnceCell<String>,
    stats: OnceCell<TextStats>,
}

/// Une copie d'élément (envoi à la base, recopie…) repart d'un cache vide :
/// inutile de dupliquer la clé de recherche, aussi longue que le texte.
impl Clone for ItemCache {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl PartialEq for ItemCache {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl std::fmt::Debug for ItemCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ItemCache")
    }
}

/// Forme de recherche : minuscules et sans accents (« École » → « ecole »).
pub fn fold(s: &str) -> String {
    use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};
    s.nfd()
        .filter(|c| !is_combining_mark(*c))
        .flat_map(char::to_lowercase)
        .collect()
}

impl Item {
    pub fn text(text: impl Into<String>, copied_at: u64, sensitive: bool) -> Self {
        Item {
            text: text.into(),
            copied_at,
            sensitive,
            ..Default::default()
        }
    }

    /// Texte de recherche (voir `fold`), calculé une seule fois.
    pub fn search_key(&self) -> &str {
        self.cache.search.get_or_init(|| match self.kind {
            // Une image se retrouve en cherchant « image » (ou le mot dans la langue de
            // l'interface) ou ses dimensions.
            ItemKind::Image => fold(&format!("image {} {}", t!("row.image"), self.meta)),
            _ => fold(&self.text),
        })
    }

    /// Nombre de lignes et de caractères, calculés une seule fois.
    pub fn stats(&self) -> TextStats {
        *self.cache.stats.get_or_init(|| {
            let t = self.text.trim();
            TextStats {
                lines: t.lines().count().max(1),
                chars: self.text.chars().count(),
                single_word: !t.contains(char::is_whitespace),
            }
        })
    }

    /// Chemins d'un élément « fichiers ».
    pub fn paths(&self) -> Vec<PathBuf> {
        self.text
            .lines()
            .filter(|l| !l.is_empty())
            .map(PathBuf::from)
            .collect()
    }
}

/// Limites de l'historique.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limits {
    pub max_items: usize,
    pub max_images: usize,
    pub max_text_bytes: usize,
    /// Taille cumulée maximale des textes (le plus récent est toujours gardé).
    pub max_total_bytes: usize,
    /// 0 = conservation illimitée.
    pub retention_days: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_items: MAX_ITEMS,
            max_images: MAX_IMAGES,
            max_text_bytes: MAX_ITEM_BYTES,
            max_total_bytes: MAX_TOTAL_BYTES,
            retention_days: 0,
        }
    }
}

#[derive(Default)]
pub struct History {
    items: Vec<Item>,
    limits: Limits,
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
            limits: Limits::default(),
        }
    }

    /// Change les limites et les applique tout de suite ; renvoie le nombre
    /// d'éléments supprimés.
    pub fn set_limits(&mut self, limits: Limits, now: u64) -> usize {
        self.limits = limits;
        let before = self.items.len();
        self.expire(now);
        self.evict();
        let removed = before - self.items.len();
        if removed > 0 {
            self.version += 1;
        }
        removed
    }

    /// Nombre d'éléments que supprimerait l'application de ces limites (aperçu).
    pub fn would_remove(&self, limits: Limits, now: u64) -> usize {
        let (kept, _) = partition(&self.items, limits, now);
        self.items.len() - kept
    }

    /// Supprime les éléments plus anciens que la durée de conservation.
    pub fn expire(&mut self, now: u64) -> usize {
        let Some(cutoff) = cutoff(self.limits, now) else {
            return 0;
        };
        let (old, keep): (Vec<Item>, Vec<Item>) = std::mem::take(&mut self.items)
            .into_iter()
            .partition(|it| it.copied_at < cutoff);
        self.items = keep;
        if let Some(store) = &self.store {
            for item in &old {
                store.delete(item);
            }
        }
        if !old.is_empty() {
            self.version += 1;
        }
        old.len()
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
    #[cfg(test)]
    pub fn push(&mut self, text: String) -> bool {
        self.push_with(text, false)
    }

    /// Ajoute un texte en tête. Un doublon est remonté en tête au lieu d'être dupliqué
    /// (et reste masqué s'il l'était). Renvoie `true` si l'historique a changé.
    #[cfg(test)]
    pub fn push_with(&mut self, text: String, sensitive: bool) -> bool {
        self.push_item(Item::text(text, now_secs(), sensitive))
    }

    /// Ajoute un élément (texte, image, fichiers) en tête ; un doublon (même nature,
    /// même contenu) est remonté au lieu d'être dupliqué.
    pub fn push_item(&mut self, new: Item) -> bool {
        if new.text.trim().is_empty() || new.text.len() > self.limits.max_text_bytes {
            return false;
        }
        let same = |it: &Item| it.kind == new.kind && it.text == new.text;
        match self.items.iter().position(same) {
            Some(0) if !new.sensitive || self.items[0].sensitive => return false,
            Some(pos) => {
                let mut item = self.items.remove(pos);
                item.copied_at = now_secs();
                item.sensitive |= new.sensitive;
                if !new.meta.is_empty() && new.meta != item.meta {
                    item.meta = new.meta;
                    item.cache = ItemCache::default();
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

    /// Applique les limites de nombre (éléments, dont images) et de taille totale.
    fn evict(&mut self) {
        let mut evicted = Vec::new();
        let mut budget = Budget::new(self.limits);
        let mut kept = Vec::with_capacity(self.items.len());
        for item in std::mem::take(&mut self.items) {
            if budget.admit(&item) {
                kept.push(item);
            } else {
                evicted.push(item);
            }
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
        self.items[index].cache = ItemCache::default();
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
    /// (sans tenir compte de la casse ni des accents). Une requête vide renvoie tout.
    /// Les éléments masqués (mots de passe) ne sont jamais retrouvés par la recherche.
    pub fn search(&self, query: &str) -> Vec<usize> {
        let words: Vec<String> = query.split_whitespace().map(fold).collect();
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
                let hay = it.search_key();
                words.iter().all(|w| hay.contains(w.as_str()))
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Vrai si l'image (par empreinte) est encore dans l'historique.
    pub fn has_image(&self, hash: &str) -> bool {
        self.items
            .iter()
            .any(|it| it.kind == ItemKind::Image && it.text == hash)
    }

    /// Met à jour la description d'une image (une fois son fichier écrit).
    pub fn set_image_meta(&mut self, hash: &str, meta: String) -> bool {
        let Some(item) = self
            .items
            .iter_mut()
            .find(|it| it.kind == ItemKind::Image && it.text == hash)
        else {
            return false;
        };
        if item.meta == meta {
            return false;
        }
        item.meta = meta;
        item.cache = ItemCache::default();
        if let Some(store) = &self.store {
            store.upsert(item);
        }
        self.version += 1;
        true
    }

    /// Retire une image (par empreinte), par exemple si son fichier n'a pas pu être écrit.
    pub fn remove_image(&mut self, hash: &str) -> bool {
        match self
            .items
            .iter()
            .position(|it| it.kind == ItemKind::Image && it.text == hash)
        {
            Some(i) => self.remove(i).is_some(),
            None => false,
        }
    }
}

/// Décompte des limites, du plus récent au plus ancien (partagé par `evict` et
/// l'aperçu `would_remove`, pour qu'ils suivent exactement la même règle).
struct Budget {
    limits: Limits,
    kept: usize,
    images: usize,
    bytes: usize,
}

impl Budget {
    fn new(limits: Limits) -> Self {
        Self {
            limits,
            kept: 0,
            images: 0,
            bytes: 0,
        }
    }

    /// Vrai si l'élément tient encore dans les limites (il est alors compté).
    fn admit(&mut self, item: &Item) -> bool {
        let is_image = item.kind == ItemKind::Image;
        let len = item.text.len();
        let over_total = self.kept > 0 && self.bytes + len > self.limits.max_total_bytes;
        if self.kept >= self.limits.max_items
            || (is_image && self.images >= self.limits.max_images)
            || over_total
        {
            return false;
        }
        self.kept += 1;
        self.images += usize::from(is_image);
        self.bytes += len;
        true
    }
}

fn cutoff(limits: Limits, now: u64) -> Option<u64> {
    (limits.retention_days > 0)
        .then(|| now.saturating_sub(u64::from(limits.retention_days) * 86_400))
}

/// Combien d'éléments resteraient avec ces limites (même règle que `evict` + `expire`).
fn partition(items: &[Item], limits: Limits, now: u64) -> (usize, usize) {
    let cutoff = cutoff(limits, now);
    let mut budget = Budget::new(limits);
    for it in items {
        if cutoff.is_some_and(|c| it.copied_at < c) {
            continue;
        }
        budget.admit(it);
    }
    (budget.kept, budget.images)
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

/// Durée relative dans la langue de l'interface (« à l'instant », « 5 min ago »…).
pub fn relative_time(then: u64, now: u64) -> String {
    let d = now.saturating_sub(then);
    match d {
        0..=59 => t!("time.now").to_string(),
        60..=3_599 => t!("time.minutes", n = d / 60),
        3_600..=86_399 => t!("time.hours", n = d / 3_600),
        _ => t!("time.days", n = d / 86_400),
    }
}

/// Aperçu sur une ligne : première ligne, tronquée.
pub fn preview(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    let first = trimmed.lines().next().unwrap_or("").trim();
    let mut out: String = first.chars().take(max_chars).collect();
    // `nth` plutôt que `count` : ne parcourt pas toute une ligne de plusieurs Mo.
    if first.chars().nth(max_chars).is_some() {
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
        assert!(
            h.search("motdepasse").is_empty(),
            "jamais trouvé par la recherche"
        );
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
        let img = |hash: &str| Item {
            text: hash.into(),
            kind: ItemKind::Image,
            meta: "1 × 1 · 1 o".into(),
            ..Default::default()
        };
        assert!(h.push_item(img("aaa")));
        assert!(h.push_item(Item {
            text: "/tmp/a.mov".into(),
            kind: ItemKind::Files,
            ..Default::default()
        }));
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
        let images = h
            .items()
            .iter()
            .filter(|i| i.kind == ItemKind::Image)
            .count();
        assert_eq!(images, MAX_IMAGES, "au plus {MAX_IMAGES} images");
        assert!(
            h.items().iter().any(|i| i.kind == ItemKind::Files),
            "le reste est conservé"
        );
        assert_eq!(h.items()[0].text, format!("h{}", MAX_IMAGES + 4));
    }

    #[test]
    fn limits_can_be_changed_with_preview() {
        let mut h = h();
        for i in 0..20 {
            h.push(format!("t{i}"));
        }
        let img = |x: &str| Item {
            text: x.into(),
            kind: ItemKind::Image,
            ..Default::default()
        };
        h.push_item(img("i1"));
        h.push_item(img("i2"));
        let now = now_secs();
        let small = Limits {
            max_items: 10,
            max_images: 1,
            ..Limits::default()
        };
        assert_eq!(
            h.would_remove(small, now),
            12,
            "aperçu : 22 → 10 (dont 1 image)"
        );
        assert_eq!(h.len(), 22, "l'aperçu ne supprime rien");
        assert_eq!(h.set_limits(small, now), 12);
        assert_eq!(h.len(), 10);
        assert_eq!(
            h.items()
                .iter()
                .filter(|i| i.kind == ItemKind::Image)
                .count(),
            1
        );
        assert_eq!(h.items()[0].text, "i2", "les plus récents sont gardés");
        // Nouvelle limite appliquée aux ajouts suivants.
        h.push("nouveau".into());
        assert_eq!(h.len(), 10);
        // Taille de texte.
        h.set_limits(
            Limits {
                max_text_bytes: 5,
                ..small
            },
            now,
        );
        assert!(!h.push("123456".into()));
        assert!(h.push("12345".into()));
    }

    #[test]
    fn retention_expires_old_items() {
        let mut h = h();
        h.push("récent".into());
        h.push("ancien".into());
        // Vieillir « ancien » de 40 jours.
        let now = now_secs();
        h.items[0].copied_at = now - 40 * 86_400;
        let month = Limits {
            retention_days: 30,
            ..Limits::default()
        };
        assert_eq!(h.would_remove(month, now), 1);
        assert_eq!(h.set_limits(month, now), 1);
        assert_eq!(h.items()[0].text, "récent");
        assert_eq!(h.expire(now + 31 * 86_400), 1, "expire aussi plus tard");
        assert_eq!(h.len(), 0);
        // 0 = illimité.
        h.push("x".into());
        h.items[0].copied_at = 0;
        assert_eq!(h.set_limits(Limits::default(), now), 0);
    }

    #[test]
    fn search_ignores_accents_and_case() {
        let mut h = h();
        h.push("École maternelle".into());
        h.push("Crème brûlée".into());
        h.push("ecole du soir".into());
        assert_eq!(h.search("ecole").len(), 2);
        assert_eq!(h.search("ÉCOLE").len(), 2);
        assert_eq!(h.search("creme brulee"), vec![1]);
        assert_eq!(h.search("brûl"), vec![1]);
        assert_eq!(fold("Ça Va Été"), "ca va ete");
    }

    #[test]
    fn cached_stats_and_search_key_follow_changes() {
        let mut h = h();
        h.push("deux\nlignes".into());
        let s = h.items()[0].stats();
        assert_eq!((s.lines, s.chars, s.single_word), (2, 11, false));
        // Une copie part d'un cache vide (pas de clé dupliquée inutilement).
        let copy = h.items()[0].clone();
        assert_eq!(copy.stats(), s);
        // Image : la description change une fois le fichier écrit.
        h.push_item(Item {
            text: "abc".into(),
            kind: ItemKind::Image,
            meta: "4 × 4".into(),
            ..Default::default()
        });
        assert!(h.search("Ko").is_empty());
        assert!(h.set_image_meta("abc", "4 × 4 · 2 Ko".into()));
        assert_eq!(h.search("ko").len(), 1, "clé de recherche recalculée");
        assert!(!h.set_image_meta("abc", "4 × 4 · 2 Ko".into()), "inchangé");
        assert!(h.has_image("abc"));
        assert!(h.remove_image("abc"));
        assert!(!h.has_image("abc"));
        assert!(!h.set_image_meta("abc", "x".into()));
    }

    #[test]
    fn total_size_cap_evicts_oldest_but_keeps_newest() {
        let mut h = h();
        let cap = Limits {
            max_total_bytes: 25,
            ..Limits::default()
        };
        h.set_limits(cap, now_secs());
        h.push("a".repeat(10));
        h.push("b".repeat(10));
        assert_eq!(h.len(), 2);
        h.push("c".repeat(10)); // 30 > 25 : le plus ancien part
        let texts: Vec<_> = h
            .items()
            .iter()
            .map(|i| i.text.chars().next().unwrap())
            .collect();
        assert_eq!(texts, vec!['c', 'b']);
        // L'aperçu suit la même règle (et ne supprime rien).
        let tight = Limits {
            max_total_bytes: 5,
            ..cap
        };
        assert_eq!(h.would_remove(tight, now_secs()), 1);
        assert_eq!(h.len(), 2);
        // Un texte plus gros que le plafond reste gardé seul (le plus récent).
        h.push("d".repeat(40));
        assert_eq!(h.len(), 1);
        h.push("e".repeat(10));
        assert_eq!(h.len(), 1, "40 + 10 > 25 : « d » part à son tour");
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
            let (store, items) =
                Store::open(db.clone(), None, None, Box::new(move || Ok(key))).unwrap();
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
