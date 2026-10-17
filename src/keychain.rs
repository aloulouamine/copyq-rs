//! Clé de chiffrement conservée dans le trousseau macOS (élément « mot de passe
//! d'application » : service `com.amine.copyq-rs`, compte `history-encryption-key`).
//! Elle n'est jamais écrite sur disque par l'app.

use crate::crypto::{self, Key};

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
