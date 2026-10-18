//! Clé de chiffrement conservée dans le trousseau macOS (élément « mot de passe
//! d'application » : service `com.amine.copyq-rs`, compte `history-encryption-key`).
//! Elle n'est jamais écrite sur disque par l'app.

use crate::crypto::{self, Cipher, Key};
use std::sync::{Arc, OnceLock};

/// Clé lue une seule fois par lancement : une autorisation du trousseau suffit pour
/// la base (mots de passe) et les fichiers (images).
static KEY: OnceLock<Result<Key, String>> = OnceLock::new();
static CIPHER: OnceLock<Result<Arc<Cipher>, String>> = OnceLock::new();

/// Clé partagée (peut afficher une demande d'autorisation du trousseau : à éviter
/// sur le fil de l'interface).
pub fn shared_key() -> Result<Key, String> {
    KEY.get_or_init(load_or_create_key).clone()
}

/// Chiffreur partagé, construit au premier besoin.
pub fn shared_cipher() -> Result<Arc<Cipher>, String> {
    CIPHER
        .get_or_init(|| shared_key().map(|k| Arc::new(Cipher::new(&k))))
        .clone()
}

/// Chiffreur s'il est déjà prêt, sans jamais solliciter le trousseau (fil de l'interface).
pub fn cipher_if_ready() -> Option<Arc<Cipher>> {
    CIPHER.get().and_then(|r| r.as_ref().ok().cloned())
}

/// Erreur du trousseau, si la clé a été demandée et refusée.
pub fn key_error() -> Option<String> {
    KEY.get().and_then(|r| r.as_ref().err().cloned())
}

pub const SERVICE: &str = "com.amine.copyq-rs";
pub const ACCOUNT: &str = "history-encryption-key";

/// Lit la clé ; la crée (et l'enregistre dans le trousseau) si elle n'existe pas.
#[cfg(target_os = "macos")]
pub fn load_or_create_key() -> Result<Key, String> {
    use security_framework::passwords::{get_generic_password, set_generic_password};
    const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;

    match get_generic_password(SERVICE, ACCOUNT) {
        Ok(bytes) => bytes
            .as_slice()
            .try_into()
            .map_err(|_| format!("clé du trousseau invalide ({} octets)", bytes.len())),
        Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => {
            let key = crypto::generate_key();
            set_generic_password(SERVICE, ACCOUNT, &key)
                .map_err(|e| format!("enregistrement de la clé dans le trousseau : {e}"))?;
            log::info!("nouvelle clé de chiffrement créée dans le trousseau");
            Ok(key)
        }
        Err(e) => Err(format!("trousseau inaccessible : {e}")),
    }
}

#[cfg(not(target_os = "macos"))]
pub fn load_or_create_key() -> Result<Key, String> {
    let _ = crypto::generate_key;
    Err("trousseau disponible uniquement sur macOS".into())
}

/// Auto-test lancé par la CI : `copyq-rs --keychain-selftest`.
pub fn selftest() -> i32 {
    let first = load_or_create_key();
    let second = load_or_create_key();
    match (&first, &second) {
        (Ok(a), Ok(b)) if a == b => {
            let c = crypto::Cipher::new(a);
            let enc = c.encrypt("test").unwrap_or_default();
            let ok = c.decrypt(&enc).as_deref() == Ok("test");
            println!(
                "clé lue deux fois à l'identique ; chiffrement : {}",
                if ok { "OK" } else { "ÉCHEC" }
            );
            println!("{}", if ok { "SUCCÈS" } else { "ÉCHEC" });
            i32::from(!ok)
        }
        _ => {
            println!(
                "ÉCHEC : {first:?} / {second:?}",
                first = first.as_ref().map(|_| "clé"),
                second = second.as_ref().map(|_| "clé")
            );
            1
        }
    }
}
