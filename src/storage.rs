//! Persistance de l'historique dans une base Turso (réécriture de SQLite en Rust,
//! fichier compatible SQLite).
//!
//! Toute la base vit dans un thread dédié : l'interface et la surveillance du
//! presse-papiers lui envoient des commandes par canal et ne sont jamais bloquées
//! par une écriture disque. Tout accès SQL est confiné à ce module, ce qui permet
//! de changer de moteur (rusqlite…) sans toucher au reste.
//!
//! Les mots de passe (`Item::sensitive`) sont chiffrés avant d'être écrits
//! (voir `crypto`) avec une clé obtenue *à la demande* auprès d'un fournisseur
//! (le trousseau macOS en production). Sans clé, un mot de passe n'est jamais
//! écrit en clair : il reste en mémoire seulement.

use crate::crypto::{self, Cipher, Key};
use crate::history::{Item, ItemKind};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;

const LOAD_LIMIT: usize = crate::history::MAX_ITEMS;

/// Fournit la clé de chiffrement (appelé au plus une fois, au premier besoin).
pub type KeyProvider = Box<dyn FnMut() -> Result<Key, String> + Send>;

enum Cmd {
    Upsert(Item),
    Delete(Item),
    Clear,
    Shutdown(Sender<()>),
}

/// Poignée vers le thread de la base. Clonable, envoi non bloquant.
#[derive(Clone)]
pub struct Store {
    tx: Sender<Cmd>,
}

static GLOBAL: OnceLock<Store> = OnceLock::new();
static ENCRYPTION_ERROR: OnceLock<String> = OnceLock::new();

/// Raison pour laquelle le chiffrement est indisponible (trousseau refusé…), le cas échéant.
pub fn encryption_error() -> Option<&'static str> {
    ENCRYPTION_ERROR.get().map(String::as_str)
}

impl Store {
    /// Ouvre (ou crée) la base, importe l'ancien `history.json` s'il existe et
    /// renvoie les éléments (déchiffrés), du plus récent au plus ancien.
    pub fn open(
        db_path: PathBuf,
        legacy_json: Option<PathBuf>,
        images_dir: Option<PathBuf>,
        keys: KeyProvider,
    ) -> Result<(Store, Vec<Item>), String> {
        if let Some(dir) = db_path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let (tx, rx) = mpsc::channel::<Cmd>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<Vec<Item>, String>>();
        thread::Builder::new()
            .name("storage".into())
            .spawn(move || worker(db_path, legacy_json, images_dir, keys, rx, ready_tx))
            .map_err(|e| e.to_string())?;
        // Délai généreux : le trousseau peut demander une autorisation à l'utilisateur.
        let items = ready_rx
            .recv_timeout(Duration::from_secs(120))
            .map_err(|_| "la base ne répond pas".to_string())??;
        let store = Store { tx };
        let _ = GLOBAL.set(store.clone());
        Ok((store, items))
    }

    pub fn upsert(&self, item: &Item) {
        let _ = self.tx.send(Cmd::Upsert(item.clone()));
    }

    pub fn delete(&self, item: &Item) {
        let _ = self.tx.send(Cmd::Delete(item.clone()));
    }

    pub fn clear(&self) {
        let _ = self.tx.send(Cmd::Clear);
    }

    /// Termine les écritures, ferme la base et arrête le thread.
    pub fn close(&self, timeout: Duration) -> bool {
        let (ack_tx, ack_rx) = mpsc::channel();
        if self.tx.send(Cmd::Shutdown(ack_tx)).is_err() {
            return false;
        }
        ack_rx.recv_timeout(timeout).is_ok()
    }
}

/// À appeler avant de quitter l'application : écrit tout et ferme la base.
pub fn close_global() {
    if let Some(store) = GLOBAL.get() {
        if !store.close(Duration::from_secs(2)) {
            log::warn!("écritures en attente non confirmées à la fermeture");
        }
    }
}

pub fn default_db_path() -> PathBuf {
    crate::history::data_dir().join("history.db")
}

fn worker(
    db_path: PathBuf,
    legacy_json: Option<PathBuf>,
    images_dir: Option<PathBuf>,
    keys: KeyProvider,
    rx: Receiver<Cmd>,
    ready: Sender<Result<Vec<Item>, String>>,
) {
    let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => {
            let _ = ready.send(Err(e.to_string()));
            return;
        }
    };
    let mut db = match rt.block_on(Db::open(&db_path, keys)) {
        Ok(db) => db,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    match rt.block_on(db.load_initial(&db_path, legacy_json.as_deref())) {
        Ok(items) => {
            let _ = ready.send(Ok(items));
        }
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    }
    let mut shutdown_ack = None;
    for cmd in rx.iter() {
        let result = match cmd {
            Cmd::Upsert(item) => rt.block_on(db.upsert(&item)),
            Cmd::Delete(item) => {
                let r = rt.block_on(db.delete(&item));
                if item.kind == ItemKind::Image {
                    if let Some(dir) = &images_dir {
                        crate::media::remove(dir, &item.text);
                    }
                }
                r
            }
            Cmd::Clear => {
                let r = rt.block_on(db.exec("DELETE FROM items"));
                if let Some(dir) = &images_dir {
                    crate::media::remove_all(dir);
                }
                r
            }
            Cmd::Shutdown(ack) => {
                shutdown_ack = Some(ack);
                break;
            }
        };
        if let Err(e) = result {
            log::error!("écriture en base échouée : {e}");
        }
    }
    drop(db);
    drop(rt);
    if let Some(ack) = shutdown_ack {
        let _ = ack.send(());
    }
}

struct Db {
    conn: turso::Connection,
    keys: KeyProvider,
    /// Résultat mis en cache : on ne redemande pas la clé (ni l'autorisation) à chaque fois.
    cipher: Option<Result<Cipher, String>>,
}

/// Ligne brute lue en base.
struct Row {
    id: i64,
    text: String,
    copied_at: u64,
    sensitive: bool,
    kind: ItemKind,
    meta: String,
}

impl Db {
    async fn open(path: &Path, keys: KeyProvider) -> Result<Db, String> {
        let path_str = path.to_str().ok_or("chemin de base non UTF-8")?;
        let database = turso::Builder::new_local(path_str)
            .build()
            .await
            .map_err(|e| format!("ouverture de {path_str} : {e}"))?;
        let conn = database.connect().map_err(|e| e.to_string())?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS items (
                id INTEGER PRIMARY KEY,
                text TEXT NOT NULL,
                copied_at INTEGER NOT NULL,
                sensitive INTEGER NOT NULL DEFAULT 0,
                fingerprint TEXT,
                kind TEXT NOT NULL DEFAULT 'text',
                meta TEXT NOT NULL DEFAULT ''
            )",
            (),
        )
        .await
        .map_err(|e| format!("création du schéma : {e}"))?;
        let db = Db { conn, keys, cipher: None };
        db.add_column_if_missing("sensitive", "INTEGER NOT NULL DEFAULT 0").await?;
        db.add_column_if_missing("fingerprint", "TEXT").await?;
        db.add_column_if_missing("kind", "TEXT NOT NULL DEFAULT 'text'").await?;
        db.add_column_if_missing("meta", "TEXT NOT NULL DEFAULT ''").await?;
        Ok(db)
    }

    async fn exec(&self, sql: &str) -> Result<(), String> {
        self.conn.execute(sql, ()).await.map(|_| ()).map_err(|e| e.to_string())
    }

    /// Bases créées par une version antérieure : ajoute la colonne manquante.
    async fn add_column_if_missing(&self, column: &str, decl: &str) -> Result<(), String> {
        let probe = format!("SELECT {column} FROM items LIMIT 1");
        if self.conn.query(&probe, ()).await.is_ok() {
            return Ok(());
        }
        self.exec(&format!("ALTER TABLE items ADD COLUMN {column} {decl}"))
            .await
            .map_err(|e| format!("mise à jour du schéma ({column}) : {e}"))?;
        log::info!("schéma mis à jour : colonne « {column} » ajoutée");
        Ok(())
    }

    /// Chiffreur, créé au premier besoin (accès au trousseau).
    fn cipher(&mut self) -> Result<&Cipher, String> {
        if self.cipher.is_none() {
            let result = (self.keys)().map(|k| Cipher::new(&k));
            if let Err(e) = &result {
                log::error!("chiffrement indisponible : {e}");
                let _ = ENCRYPTION_ERROR.set(e.clone());
            }
            self.cipher = Some(result);
        }
        match self.cipher.as_ref() {
            Some(Ok(c)) => Ok(c),
            Some(Err(e)) => Err(e.clone()),
            None => unreachable!(),
        }
    }

    async fn load_initial(&mut self, path: &Path, legacy_json: Option<&Path>) -> Result<Vec<Item>, String> {
        let mut items = self.load().await?;
        if items.is_empty() {
            if let Some(json) = legacy_json {
                if self.migrate_json(json).await? > 0 {
                    items = self.load().await?;
                }
            }
        }
        log::info!("base ouverte : {} éléments ({})", items.len(), path.display());
        Ok(items)
    }

    async fn rows(&self) -> Result<Vec<Row>, String> {
        let sql = format!(
            "SELECT id, text, copied_at, sensitive, kind, meta FROM items ORDER BY copied_at DESC, id DESC LIMIT {LOAD_LIMIT}"
        );
        let mut rows = self.conn.query(&sql, ()).await.map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await.map_err(|e| e.to_string())? {
            let int = |i: usize| match row.get_value(i) {
                Ok(turso::Value::Integer(v)) => Some(v),
                _ => None,
            };
            let text_at = |i: usize| match row.get_value(i) {
                Ok(turso::Value::Text(t)) => Some(t),
                _ => None,
            };
            let Some(text) = text_at(1) else { continue };
            out.push(Row {
                id: int(0).unwrap_or(0),
                text,
                copied_at: int(2).unwrap_or(0).max(0) as u64,
                sensitive: int(3).unwrap_or(0) != 0,
                kind: ItemKind::parse(text_at(4).as_deref().unwrap_or("text")),
                meta: text_at(5).unwrap_or_default(),
            });
        }
        Ok(out)
    }

    /// Charge les éléments, déchiffre les mots de passe et chiffre ceux qu'une
    /// version précédente avait stockés en clair.
    async fn load(&mut self) -> Result<Vec<Item>, String> {
        let mut items = Vec::new();
        let mut undecryptable = 0;
        let mut upgraded = 0;
        for row in self.rows().await? {
            let text = if crypto::is_encrypted(&row.text) {
                match self.cipher().and_then(|c| c.decrypt(&row.text)) {
                    Ok(plain) => plain,
                    Err(_) => {
                        undecryptable += 1;
                        continue;
                    }
                }
            } else {
                if row.sensitive {
                    // Mot de passe en clair d'une version antérieure : on le chiffre.
                    let sealed = self
                        .cipher()
                        .and_then(|c| Ok((c.encrypt(&row.text)?, c.fingerprint(&row.text))));
                    if let Ok((enc, fp)) = sealed {
                        let sql = format!(
                            "UPDATE items SET text = ?1, fingerprint = ?2 WHERE id = {}",
                            row.id
                        );
                        self.conn
                            .execute(&sql, [enc.as_str(), fp.as_str()])
                            .await
                            .map_err(|e| e.to_string())?;
                        upgraded += 1;
                    }
                }
                row.text
            };
            items.push(Item {
                text,
                copied_at: row.copied_at,
                sensitive: row.sensitive,
                kind: row.kind,
                meta: row.meta,
            });
        }
        if upgraded > 0 {
            log::info!("{upgraded} mot(s) de passe stocké(s) en clair désormais chiffré(s)");
            // Purge les anciennes pages contenant le texte en clair (si pris en charge).
            if let Err(e) = self.exec("VACUUM").await {
                log::warn!("VACUUM indisponible ({e}) : d'anciennes données peuvent subsister");
            }
        }
        if undecryptable > 0 {
            log::warn!("{undecryptable} mot(s) de passe non déchiffrable(s) (clé absente ou différente) : masqués");
        }
        Ok(items)
    }

    async fn upsert(&mut self, item: &Item) -> Result<(), String> {
        // Préparer la ligne : chiffrée pour un mot de passe, en clair sinon.
        let (stored, fingerprint) = if item.sensitive {
            let c = self
                .cipher()
                .map_err(|e| format!("mot de passe gardé en mémoire seulement : {e}"))?;
            (c.encrypt(&item.text)?, Some(c.fingerprint(&item.text)))
        } else {
            (item.text.clone(), None)
        };

        // Supprimer puis réinsérer : l'élément prend un nouvel id, donc remonte en tête.
        self.exec("BEGIN").await?;
        let result = async {
            match &fingerprint {
                Some(fp) => {
                    self.conn
                        .execute("DELETE FROM items WHERE fingerprint = ?1", [fp.as_str()])
                        .await
                        .map_err(|e| e.to_string())?;
                    // Ancienne copie en clair du même texte (avant qu'il soit marqué).
                    self.conn
                        .execute("DELETE FROM items WHERE text = ?1", [item.text.as_str()])
                        .await
                        .map_err(|e| e.to_string())?;
                    // copied_at est un entier que nous contrôlons : pas d'injection.
                    let sql = format!(
                        "INSERT INTO items (text, copied_at, sensitive, fingerprint, kind, meta) \
                         VALUES (?1, {}, 1, ?2, ?3, ?4)",
                        item.copied_at as i64
                    );
                    self.conn
                        .execute(&sql, [stored.as_str(), fp.as_str(), item.kind.as_str(), item.meta.as_str()])
                        .await
                        .map_err(|e| e.to_string())?;
                }
                None => {
                    self.conn
                        .execute(
                            "DELETE FROM items WHERE text = ?1 AND kind = ?2",
                            [stored.as_str(), item.kind.as_str()],
                        )
                        .await
                        .map_err(|e| e.to_string())?;
                    let sql = format!(
                        "INSERT INTO items (text, copied_at, sensitive, kind, meta) VALUES (?1, {}, 0, ?2, ?3)",
                        item.copied_at as i64
                    );
                    self.conn
                        .execute(&sql, [stored.as_str(), item.kind.as_str(), item.meta.as_str()])
                        .await
                        .map_err(|e| e.to_string())?;
                }
            }
            Ok::<(), String>(())
        }
        .await;
        self.exec(if result.is_ok() { "COMMIT" } else { "ROLLBACK" }).await?;
        result
    }

    async fn delete(&mut self, item: &Item) -> Result<(), String> {
        if item.sensitive {
            if let Ok(fp) = self.cipher().map(|c| c.fingerprint(&item.text)) {
                self.conn
                    .execute("DELETE FROM items WHERE fingerprint = ?1", [fp.as_str()])
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }
        // Éléments ordinaires, ou mot de passe resté en clair (version antérieure).
        self.conn
            .execute(
                "DELETE FROM items WHERE text = ?1 AND kind = ?2",
                [item.text.as_str(), item.kind.as_str()],
            )
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    /// Importe l'ancien fichier JSON (du plus ancien au plus récent), puis le renomme.
    async fn migrate_json(&mut self, json: &Path) -> Result<usize, String> {
        let Some(mut legacy) = crate::history::read_legacy_json(json) else {
            return Ok(0);
        };
        legacy.reverse();
        for item in &legacy {
            if let Err(e) = self.upsert(item).await {
                log::warn!("élément non importé : {e}");
            }
        }
        let done = json.with_extension("json.migrated");
        if let Err(e) = std::fs::rename(json, &done) {
            log::warn!("migration faite mais {} non renommé : {e}", json.display());
        }
        log::info!("{} éléments importés depuis {}", legacy.len(), json.display());
        Ok(legacy.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(k: Key) -> KeyProvider {
        Box::new(move || Ok(k))
    }

    fn file_bytes(db: &Path) -> Vec<u8> {
        let mut all = std::fs::read(db).unwrap_or_default();
        for suffix in ["-wal", "-shm"] {
            let mut p = db.as_os_str().to_owned();
            p.push(suffix);
            all.extend(std::fs::read(PathBuf::from(p)).unwrap_or_default());
        }
        all
    }

    fn contains(haystack: &[u8], needle: &str) -> bool {
        haystack.windows(needle.len()).any(|w| w == needle.as_bytes())
    }

    fn secret(text: &str, at: u64) -> Item {
        Item::text(text, at, true)
    }

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("copyq-rs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn item(text: &str, at: u64) -> Item {
        Item::text(text, at, false)
    }

    #[test]
    fn roundtrip_upsert_delete_clear() {
        let k = crypto::generate_key();
        let dir = temp_dir("roundtrip");
        let db = dir.join("h.db");
        {
            let (store, items) = Store::open(db.clone(), None, None, keys(k)).unwrap();
            assert!(items.is_empty());
            store.upsert(&item("a", 1));
            store.upsert(&item("b", 2));
            store.upsert(&item("a", 3)); // doublon : remonte en tête
            store.upsert(&item("c", 4));
            store.delete(&item("c", 0));
            assert!(store.close(Duration::from_secs(5)));
        }
        let (store, items) = Store::open(db.clone(), None, None, keys(k)).unwrap();
        let texts: Vec<_> = items.iter().map(|i| i.text.as_str()).collect();
        assert_eq!(texts, vec!["a", "b"]);
        assert_eq!(items[0].copied_at, 3);

        store.clear();
        assert!(store.close(Duration::from_secs(5)));
        let (store, items) = Store::open(db, None, None, keys(k)).unwrap();
        assert!(items.is_empty());
        store.close(Duration::from_secs(5));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn special_characters_survive() {
        let k = crypto::generate_key();
        let dir = temp_dir("chars");
        let db = dir.join("h.db");
        let tricky = "l'apostrophe \"guillemets\" ; DROP TABLE items; -- 🎉\nligne 2\0fin";
        {
            let (store, _) = Store::open(db.clone(), None, None, keys(k)).unwrap();
            store.upsert(&item(tricky, 1));
            assert!(store.close(Duration::from_secs(5)));
        }
        let (store, items) = Store::open(db, None, None, keys(k)).unwrap();
        store.close(Duration::from_secs(5));
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].text, tricky);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn sensitive_flag_roundtrip() {
        let k = crypto::generate_key();
        let dir = temp_dir("sensitive");
        let db = dir.join("h.db");
        {
            let (store, _) = Store::open(db.clone(), None, None, keys(k)).unwrap();
            store.upsert(&Item::text("secret", 2, true));
            store.upsert(&item("public", 3));
            assert!(store.close(Duration::from_secs(5)));
        }
        let (store, items) = Store::open(db, None, None, keys(k)).unwrap();
        store.close(Duration::from_secs(5));
        assert_eq!(items.len(), 2);
        assert!(!items[0].sensitive && items[0].text == "public");
        assert!(items[1].sensitive && items[1].text == "secret");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn upgrades_schema_of_older_database() {
        let k = crypto::generate_key();
        let dir = temp_dir("schema");
        let db = dir.join("h.db");
        // Base au format v0.1 (sans colonne `sensitive`).
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async {
            let d = turso::Builder::new_local(db.to_str().unwrap()).build().await.unwrap();
            let c = d.connect().unwrap();
            c.execute(
                "CREATE TABLE items (id INTEGER PRIMARY KEY, text TEXT NOT NULL, copied_at INTEGER NOT NULL)",
                (),
            )
            .await
            .unwrap();
            c.execute("INSERT INTO items (text, copied_at) VALUES ('ancien', 5)", ()).await.unwrap();
        });
        drop(rt);
        let (store, items) = Store::open(db.clone(), None, None, keys(k)).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].text, "ancien");
        assert!(!items[0].sensitive);
        store.upsert(&Item::text("nouveau", 6, true));
        assert!(store.close(Duration::from_secs(5)));
        let (store, items) = Store::open(db, None, None, keys(k)).unwrap();
        store.close(Duration::from_secs(5));
        assert!(items[0].sensitive);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn passwords_are_encrypted_on_disk() {
        let k = crypto::generate_key();
        let dir = temp_dir("encrypted");
        let db = dir.join("h.db");
        let pw = "S3cr3t-Unique-7f3a9c";
        {
            let (store, _) = Store::open(db.clone(), None, None, keys(k)).unwrap();
            store.upsert(&secret(pw, 1));
            store.upsert(&item("texte ordinaire visible", 2));
            assert!(store.close(Duration::from_secs(5)));
        }
        let bytes = file_bytes(&db);
        assert!(!contains(&bytes, pw), "le mot de passe ne doit jamais apparaître en clair");
        assert!(contains(&bytes, "texte ordinaire visible"));

        // Même clé : déchiffré au chargement.
        let (store, items) = Store::open(db.clone(), None, None, keys(k)).unwrap();
        store.close(Duration::from_secs(5));
        assert_eq!(items.len(), 2);
        assert_eq!(items[1].text, pw);
        assert!(items[1].sensitive);

        // Autre clé : le mot de passe est ignoré, le reste est lu.
        let (store, items) = Store::open(db, None, None, keys(crypto::generate_key())).unwrap();
        store.close(Duration::from_secs(5));
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].text, "texte ordinaire visible");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn without_key_passwords_are_never_written() {
        let k = crypto::generate_key();
        let dir = temp_dir("nokey");
        let db = dir.join("h.db");
        let pw = "Jamais-Ecrit-9b21";
        {
            let refuse: KeyProvider = Box::new(|| Err("accès au trousseau refusé".into()));
            let (store, _) = Store::open(db.clone(), None, None, refuse).unwrap();
            store.upsert(&secret(pw, 1));
            store.upsert(&item("ordinaire", 2));
            assert!(store.close(Duration::from_secs(5)));
        }
        assert!(!contains(&file_bytes(&db), pw));
        let (store, items) = Store::open(db, None, None, keys(k)).unwrap();
        store.close(Duration::from_secs(5));
        assert_eq!(items.len(), 1, "seul l'élément ordinaire est en base");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn key_is_requested_only_when_needed_and_once() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        let k = crypto::generate_key();
        let calls = Arc::new(AtomicUsize::new(0));
        let counting = |calls: Arc<AtomicUsize>| -> KeyProvider {
            Box::new(move || {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(k)
            })
        };
        let dir = temp_dir("lazy");
        let db = dir.join("h.db");
        let (store, _) = Store::open(db.clone(), None, None, counting(calls.clone())).unwrap();
        store.upsert(&item("a", 1));
        store.upsert(&item("b", 2));
        assert!(store.close(Duration::from_secs(5)));
        assert_eq!(calls.load(Ordering::SeqCst), 0, "pas de trousseau sans mot de passe");

        let (store, _) = Store::open(db, None, None, counting(calls.clone())).unwrap();
        store.upsert(&secret("x1", 3));
        store.upsert(&secret("x2", 4));
        store.delete(&secret("x1", 0));
        assert!(store.close(Duration::from_secs(5)));
        assert_eq!(calls.load(Ordering::SeqCst), 1, "clé demandée une seule fois");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn delete_and_dedup_work_on_encrypted_items() {
        let k = crypto::generate_key();
        let dir = temp_dir("fp");
        let db = dir.join("h.db");
        {
            let (store, _) = Store::open(db.clone(), None, None, keys(k)).unwrap();
            store.upsert(&secret("pw-a", 1));
            store.upsert(&secret("pw-b", 2));
            store.upsert(&secret("pw-a", 3)); // doublon chiffré : remplacé, pas dupliqué
            store.delete(&secret("pw-b", 0));
            assert!(store.close(Duration::from_secs(5)));
        }
        let (store, items) = Store::open(db, None, None, keys(k)).unwrap();
        store.close(Duration::from_secs(5));
        let texts: Vec<_> = items.iter().map(|i| i.text.as_str()).collect();
        assert_eq!(texts, vec!["pw-a"]);
        assert_eq!(items[0].copied_at, 3);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn plaintext_passwords_from_older_version_get_encrypted() {
        let k = crypto::generate_key();
        let dir = temp_dir("upgrade-pw");
        let db = dir.join("h.db");
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let raw = |sql: &'static str| {
            let db = db.clone();
            rt.block_on(async move {
                let d = turso::Builder::new_local(db.to_str().unwrap()).build().await.unwrap();
                let c = d.connect().unwrap();
                let mut out = Vec::new();
                if sql.starts_with("SELECT") {
                    let mut rows = c.query(sql, ()).await.unwrap();
                    while let Some(r) = rows.next().await.unwrap() {
                        if let Ok(turso::Value::Text(t)) = r.get_value(0) {
                            out.push(t);
                        }
                    }
                } else {
                    c.execute(sql, ()).await.unwrap();
                }
                out
            })
        };
        // Base v0.2 : mot de passe stocké en clair, pas de colonne fingerprint.
        raw("CREATE TABLE items (id INTEGER PRIMARY KEY, text TEXT NOT NULL, copied_at INTEGER NOT NULL, sensitive INTEGER NOT NULL DEFAULT 0)");
        raw("INSERT INTO items (text, copied_at, sensitive) VALUES ('ancien-en-clair', 5, 1)");

        let (store, items) = Store::open(db.clone(), None, None, keys(k)).unwrap();
        assert!(store.close(Duration::from_secs(5)));
        assert_eq!(items[0].text, "ancien-en-clair");
        assert!(items[0].sensitive);
        let stored = raw("SELECT text FROM items");
        assert_eq!(stored.len(), 1);
        assert!(crypto::is_encrypted(&stored[0]), "réécrit chiffré : {}", stored[0]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn images_and_files_roundtrip_and_image_files_are_removed() {
        let k = crypto::generate_key();
        let dir = temp_dir("media");
        let db = dir.join("h.db");
        let images = dir.join("images");
        let px: Vec<u8> = vec![200; 4 * 4 * 4];
        let saved = crate::media::save_rgba(&images, 4, 4, px).unwrap();
        let image = Item {
            text: saved.hash.clone(),
            copied_at: 5,
            kind: ItemKind::Image,
            meta: saved.meta.clone(),
            ..Default::default()
        };
        let files = Item {
            text: "/tmp/vidéo.mov\n/tmp/son.m4a".into(),
            copied_at: 6,
            kind: ItemKind::Files,
            ..Default::default()
        };
        {
            let (store, _) = Store::open(db.clone(), None, Some(images.clone()), keys(k)).unwrap();
            store.upsert(&image);
            store.upsert(&files);
            assert!(store.close(Duration::from_secs(5)));
        }
        let (store, items) = Store::open(db.clone(), None, Some(images.clone()), keys(k)).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0], files);
        assert_eq!(items[1], image);
        assert_eq!(items[0].paths().len(), 2);

        store.delete(&image);
        assert!(store.close(Duration::from_secs(5)));
        assert!(!crate::media::full_path(&images, &saved.hash).exists(), "fichier image supprimé");
        let (store, items) = Store::open(db, None, Some(images), keys(k)).unwrap();
        store.close(Duration::from_secs(5));
        assert_eq!(items, vec![files]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn migrates_legacy_json_once() {
        let k = crypto::generate_key();
        let dir = temp_dir("migrate");
        let json = dir.join("history.json");
        std::fs::write(
            &json,
            r#"{"version":1,"items":[{"text":"récent","copied_at":20},{"text":"ancien","copied_at":10}]}"#,
        )
        .unwrap();
        let db = dir.join("h.db");
        let (store, items) = Store::open(db.clone(), Some(json.clone()), None, keys(k)).unwrap();
        store.close(Duration::from_secs(5));
        let texts: Vec<_> = items.iter().map(|i| i.text.as_str()).collect();
        assert_eq!(texts, vec!["récent", "ancien"]);
        assert!(!json.exists(), "le JSON doit être renommé");
        assert!(dir.join("history.json.migrated").exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
