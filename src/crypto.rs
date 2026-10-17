//! Chiffrement des mots de passe enregistrés.
//!
//! - AES-256-GCM (chiffrement authentifié) avec un nonce aléatoire de 96 bits
//!   par élément. Format stocké : `enc:v1:` + base64(nonce ‖ texte chiffré ‖ tag).
//! - Empreinte HMAC-SHA256 du texte en clair, avec une sous-clé dérivée, pour
//!   retrouver un élément (doublon, suppression) sans le déchiffrer et sans
//!   permettre de deviner un mot de passe court sans la clé.

use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::Sha256;

const PREFIX: &str = "enc:v1:";
const NONCE_LEN: usize = 12;

pub type Key = [u8; 32];

pub fn generate_key() -> Key {
    Aes256Gcm::generate_key(OsRng).into()
}

pub fn is_encrypted(stored: &str) -> bool {
    stored.starts_with(PREFIX)
}

pub struct Cipher {
    aead: Aes256Gcm,
    fingerprint_key: [u8; 32],
}

impl Cipher {
    pub fn new(key: &Key) -> Self {
        // Sous-clé distincte pour les empreintes (séparation des usages).
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(key).expect("clé HMAC de 32 octets");
        mac.update(b"copyq-rs fingerprint v1");
        Self {
            aead: Aes256Gcm::new(key.into()),
            fingerprint_key: mac.finalize().into_bytes().into(),
        }
    }

    pub fn encrypt(&self, plain: &str) -> Result<String, String> {
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = self
            .aead
            .encrypt(&nonce, plain.as_bytes())
            .map_err(|_| "chiffrement impossible".to_string())?;
        let mut blob = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        blob.extend_from_slice(&nonce);
        blob.extend_from_slice(&ciphertext);
        Ok(format!("{PREFIX}{}", B64.encode(blob)))
    }

    pub fn decrypt(&self, stored: &str) -> Result<String, String> {
        let encoded = stored.strip_prefix(PREFIX).ok_or("format inconnu")?;
        let blob = B64.decode(encoded).map_err(|_| "base64 invalide".to_string())?;
        if blob.len() < NONCE_LEN + 16 {
            return Err("donnée tronquée".into());
        }
        let (nonce, ciphertext) = blob.split_at(NONCE_LEN);
        let plain = self
            .aead
            .decrypt(Nonce::from_slice(nonce), ciphertext)
            .map_err(|_| "déchiffrement refusé (clé différente ou donnée altérée)".to_string())?;
        String::from_utf8(plain).map_err(|_| "texte non UTF-8".to_string())
    }

    pub fn fingerprint(&self, plain: &str) -> String {
        let mut mac =
            <Hmac<Sha256> as Mac>::new_from_slice(&self.fingerprint_key).expect("clé HMAC");
        mac.update(plain.as_bytes());
        mac.finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let c = Cipher::new(&generate_key());
        let secret = "hunter2 — mot de passe accentué 🔐";
        let stored = c.encrypt(secret).unwrap();
        assert!(is_encrypted(&stored));
        assert!(!stored.contains("hunter2"));
        assert_eq!(c.decrypt(&stored).unwrap(), secret);
    }

    #[test]
    fn same_text_gives_different_ciphertexts_but_same_fingerprint() {
        let c = Cipher::new(&generate_key());
        assert_ne!(c.encrypt("abc").unwrap(), c.encrypt("abc").unwrap());
        assert_eq!(c.fingerprint("abc"), c.fingerprint("abc"));
        assert_ne!(c.fingerprint("abc"), c.fingerprint("abd"));
        assert_eq!(c.fingerprint("abc").len(), 64);
    }

    #[test]
    fn wrong_key_or_tampering_is_rejected() {
        let c1 = Cipher::new(&generate_key());
        let c2 = Cipher::new(&generate_key());
        let stored = c1.encrypt("secret").unwrap();
        assert!(c2.decrypt(&stored).is_err(), "autre clé");
        assert_ne!(c1.fingerprint("secret"), c2.fingerprint("secret"));

        let mut bytes = B64.decode(stored.strip_prefix(PREFIX).unwrap()).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        let tampered = format!("{PREFIX}{}", B64.encode(bytes));
        assert!(c1.decrypt(&tampered).is_err(), "donnée altérée");
        assert!(c1.decrypt("pas chiffré").is_err());
    }
}
