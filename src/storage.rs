//! Persistance de l'historique dans une base Turso (réécriture de SQLite en Rust,
//! fichier compatible SQLite).
//!
//! Toute la base vit dans un thread dédié : l'interface et la surveillance du
//! presse-papiers lui envoient des commandes par canal et ne sont jamais bloquées
//! par une écriture disque. Tout accès SQL est confiné à ce module, ce qui permet
//! de changer de moteur (rusqlite…) sans toucher au reste.

use crate::history::Item;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;

const LOAD_LIMIT: usize = crate::history::MAX_ITEMS;

enum Cmd {
    Upsert(Item),
    Delete(String),
    Clear,
    Shutdown(Sender<()>),
}

/// Poignée vers le thread de la base. Clonable, envoi non bloquant.
#[derive(Clone)]
pub struct Store {
    tx: Sender<Cmd>,
}

static GLOBAL: OnceLock<Store> = OnceLock::new();

impl Store {
    /// Ouvre (ou crée) la base, importe l'ancien `history.json` s'il existe et
    /// renvoie les éléments, du plus récent au plus ancien.
    pub fn open(db_path: PathBuf, legacy_json: Option<PathBuf>) -> Result<(Store, Vec<Item>), String> {
        if let Some(dir) = db_path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let (tx, rx) = mpsc::channel::<Cmd>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<Vec<Item>, String>>();
        thread::Builder::new()
            .name("storage".into())
            .spawn(move || worker(db_path, legacy_json, rx, ready_tx))
            .map_err(|e| e.to_string())?;
        let items = ready_rx
            .recv_timeout(Duration::from_secs(15))
            .map_err(|_| "la base ne répond pas".to_string())??;
        let store = Store { tx };
        let _ = GLOBAL.set(store.clone());
        Ok((store, items))
    }

    pub fn upsert(&self, item: &Item) {
        let _ = self.tx.send(Cmd::Upsert(item.clone()));
    }

    pub fn delete(&self, text: &str) {
        let _ = self.tx.send(Cmd::Delete(text.to_owned()));
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
    let conn = match rt.block_on(init(&db_path, legacy_json.as_deref())) {
        Ok((conn, items)) => {
            let _ = ready.send(Ok(items));
            conn
        }
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    let mut shutdown_ack = None;
    for cmd in rx.iter() {
        let result = match cmd {
            Cmd::Upsert(item) => rt.block_on(upsert(&conn, &item)),
            Cmd::Delete(text) => rt.block_on(delete(&conn, &text)),
            Cmd::Clear => rt
                .block_on(conn.execute("DELETE FROM items", ()))
                .map(|_| ())
                .map_err(|e| e.to_string()),
            Cmd::Shutdown(ack) => {
                shutdown_ack = Some(ack);
                break;
            }
        };
        if let Err(e) = result {
            log::error!("écriture en base échouée : {e}");
        }
    }
    drop(conn);
    drop(rt);
    if let Some(ack) = shutdown_ack {
        let _ = ack.send(());
    }
}

async fn init(path: &Path, legacy_json: Option<&Path>) -> Result<(turso::Connection, Vec<Item>), String> {
    let path_str = path.to_str().ok_or("chemin de base non UTF-8")?;
    let db = turso::Builder::new_local(path_str)
        .build()
        .await
        .map_err(|e| format!("ouverture de {path_str} : {e}"))?;
    let conn = db.connect().map_err(|e| e.to_string())?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS items (
            id INTEGER PRIMARY KEY,
            text TEXT NOT NULL,
            copied_at INTEGER NOT NULL,
            sensitive INTEGER NOT NULL DEFAULT 0
        )",
        (),
    )
    .await
    .map_err(|e| format!("création du schéma : {e}"))?;
    migrate_schema(&conn).await?;

    let mut items = load(&conn).await?;
    if items.is_empty() {
        if let Some(json) = legacy_json {
            let imported = migrate_json(&conn, json).await?;
            if imported > 0 {
                items = load(&conn).await?;
            }
        }
    }
    log::info!("base ouverte : {} éléments ({})", items.len(), path.display());
    Ok((conn, items))
}

/// Bases créées avant l'ajout de la colonne `sensitive` (v0.2) : on l'ajoute.
async fn migrate_schema(conn: &turso::Connection) -> Result<(), String> {
    if conn.query("SELECT sensitive FROM items LIMIT 1", ()).await.is_ok() {
        return Ok(());
    }
    conn.execute(
        "ALTER TABLE items ADD COLUMN sensitive INTEGER NOT NULL DEFAULT 0",
        (),
    )
    .await
    .map_err(|e| format!("mise à jour du schéma : {e}"))?;
    log::info!("schéma mis à jour : colonne « sensitive » ajoutée");
    Ok(())
}

async fn load(conn: &turso::Connection) -> Result<Vec<Item>, String> {
    let sql = format!(
        "SELECT text, copied_at, sensitive FROM items ORDER BY id DESC LIMIT {LOAD_LIMIT}"
    );
    let mut rows = conn.query(&sql, ()).await.map_err(|e| e.to_string())?;
    let mut items = Vec::new();
    while let Some(row) = rows.next().await.map_err(|e| e.to_string())? {
        let text = match row.get_value(0).map_err(|e| e.to_string())? {
            turso::Value::Text(t) => t,
            _ => continue,
        };
        let copied_at = match row.get_value(1).map_err(|e| e.to_string())? {
            turso::Value::Integer(i) => i.max(0) as u64,
            _ => 0,
        };
        let sensitive = matches!(
            row.get_value(2).map_err(|e| e.to_string())?,
            turso::Value::Integer(i) if i != 0
        );
        items.push(Item { text, copied_at, sensitive });
    }
    Ok(items)
}

async fn upsert(conn: &turso::Connection, item: &Item) -> Result<(), String> {
    // Supprimer puis réinsérer : l'élément prend un nouvel id, donc remonte en tête.
    conn.execute("BEGIN", ()).await.map_err(|e| e.to_string())?;
    let result = async {
        conn.execute("DELETE FROM items WHERE text = ?1", [item.text.as_str()])
            .await
            .map_err(|e| e.to_string())?;
        // copied_at et sensitive sont des entiers que nous contrôlons : pas d'injection.
        let sql = format!(
            "INSERT INTO items (text, copied_at, sensitive) VALUES (?1, {}, {})",
            item.copied_at as i64,
            i64::from(item.sensitive)
        );
        conn.execute(&sql, [item.text.as_str()])
            .await
            .map_err(|e| e.to_string())?;
        Ok::<(), String>(())
    }
    .await;
    let end = if result.is_ok() { "COMMIT" } else { "ROLLBACK" };
    conn.execute(end, ()).await.map_err(|e| e.to_string())?;
    result
}

async fn delete(conn: &turso::Connection, text: &str) -> Result<(), String> {
    conn.execute("DELETE FROM items WHERE text = ?1", [text])
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Importe l'ancien fichier JSON (du plus ancien au plus récent), puis le renomme.
async fn migrate_json(conn: &turso::Connection, json: &Path) -> Result<usize, String> {
    let Some(mut legacy) = crate::history::read_legacy_json(json) else {
        return Ok(0);
    };
    legacy.reverse();
    for item in &legacy {
        upsert(conn, item).await?;
    }
    let done = json.with_extension("json.migrated");
    if let Err(e) = std::fs::rename(json, &done) {
        log::warn!("migration faite mais {} non renommé : {e}", json.display());
    }
    log::info!("{} éléments importés depuis {}", legacy.len(), json.display());
    Ok(legacy.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("copyq-rs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn item(text: &str, at: u64) -> Item {
        Item { text: text.into(), copied_at: at, sensitive: false }
    }

    #[test]
    fn roundtrip_upsert_delete_clear() {
        let dir = temp_dir("roundtrip");
        let db = dir.join("h.db");
        {
            let (store, items) = Store::open(db.clone(), None).unwrap();
            assert!(items.is_empty());
            store.upsert(&item("a", 1));
            store.upsert(&item("b", 2));
            store.upsert(&item("a", 3)); // doublon : remonte en tête
            store.upsert(&item("c", 4));
            store.delete("c");
            assert!(store.close(Duration::from_secs(5)));
        }
        let (store, items) = Store::open(db.clone(), None).unwrap();
        let texts: Vec<_> = items.iter().map(|i| i.text.as_str()).collect();
        assert_eq!(texts, vec!["a", "b"]);
        assert_eq!(items[0].copied_at, 3);

        store.clear();
        assert!(store.close(Duration::from_secs(5)));
        let (store, items) = Store::open(db, None).unwrap();
        assert!(items.is_empty());
        store.close(Duration::from_secs(5));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn special_characters_survive() {
        let dir = temp_dir("chars");
        let db = dir.join("h.db");
        let tricky = "l'apostrophe \"guillemets\" ; DROP TABLE items; -- 🎉\nligne 2\0fin";
        {
            let (store, _) = Store::open(db.clone(), None).unwrap();
            store.upsert(&item(tricky, 1));
            assert!(store.close(Duration::from_secs(5)));
        }
        let (store, items) = Store::open(db, None).unwrap();
        store.close(Duration::from_secs(5));
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].text, tricky);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn sensitive_flag_roundtrip() {
        let dir = temp_dir("sensitive");
        let db = dir.join("h.db");
        {
            let (store, _) = Store::open(db.clone(), None).unwrap();
            store.upsert(&Item { text: "secret".into(), copied_at: 2, sensitive: true });
            store.upsert(&item("public", 3));
            assert!(store.close(Duration::from_secs(5)));
        }
        let (store, items) = Store::open(db, None).unwrap();
        store.close(Duration::from_secs(5));
        assert_eq!(items.len(), 2);
        assert!(!items[0].sensitive && items[0].text == "public");
        assert!(items[1].sensitive && items[1].text == "secret");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn upgrades_schema_of_older_database() {
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
        let (store, items) = Store::open(db.clone(), None).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].text, "ancien");
        assert!(!items[0].sensitive);
        store.upsert(&Item { text: "nouveau".into(), copied_at: 6, sensitive: true });
        assert!(store.close(Duration::from_secs(5)));
        let (store, items) = Store::open(db, None).unwrap();
        store.close(Duration::from_secs(5));
        assert!(items[0].sensitive);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn migrates_legacy_json_once() {
        let dir = temp_dir("migrate");
        let json = dir.join("history.json");
        std::fs::write(
            &json,
            r#"{"version":1,"items":[{"text":"récent","copied_at":20},{"text":"ancien","copied_at":10}]}"#,
        )
        .unwrap();
        let db = dir.join("h.db");
        let (store, items) = Store::open(db.clone(), Some(json.clone())).unwrap();
        store.close(Duration::from_secs(5));
        let texts: Vec<_> = items.iter().map(|i| i.text.as_str()).collect();
        assert_eq!(texts, vec!["récent", "ancien"]);
        assert!(!json.exists(), "le JSON doit être renommé");
        assert!(dir.join("history.json.migrated").exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
