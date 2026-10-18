//! Chiffrement des mots de passe enregistrés.
//!
//! - AES-256-GCM (chiffrement authentifié) avec un nonce aléatoire de 96 bits
//!   par élément. Format stocké : `enc:v1:` + base64(nonce ‖ texte chiffré ‖ tag).
//! - Empreinte HMAC-SHA256 du texte en clair, avec une sous-clé dérivée, pour
//!   retrouver un élément (doublon, suppression) sans le déchiffrer et sans
//!   permettre de deviner un mot de passe court sans la clé.
//! - Fichiers (images, vignettes) : AES-256-GCM avec une autre sous-clé dérivée.
//!   Format : `CQENC1` ‖ nonce ‖ données chiffrées ‖ tag ; le nom logique du
//!   fichier sert de données associées, pour qu'on ne puisse pas en échanger deux.

use aes_gcm::aead::{Aead, Generate, Key as AeadKey, KeyInit, Nonce, Payload};
use aes_gcm::Aes256Gcm;
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::Sha256;

const PREFIX: &str = "enc:v1:";
const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16;
/// En-tête des fichiers chiffrés.
const FILE_MAGIC: &[u8] = b"CQENC1";

pub type Key = [u8; 32];

pub fn generate_key() -> Key {
    AeadKey::<Aes256Gcm>::generate().into()
}

/// Sous-clé dérivée de la clé principale (une par usage).
fn subkey(key: &Key, label: &[u8]) -> Key {
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(key).expect("clé HMAC de 32 octets");
    mac.update(label);
    mac.finalize().into_bytes().into()
}

fn aead(key: &Key) -> Aes256Gcm {
    Aes256Gcm::new(&AeadKey::<Aes256Gcm>::from(*key))
}

fn nonce_from(bytes: &[u8]) -> Result<Nonce<Aes256Gcm>, String> {
    Nonce::<Aes256Gcm>::try_from(bytes).map_err(|_| "nonce invalide".to_string())
}

pub fn is_encrypted(stored: &str) -> bool {
    stored.starts_with(PREFIX)
}

pub struct Cipher {
    aead: Aes256Gcm,
    files: Aes256Gcm,
    fingerprint_key: Key,
}

impl Cipher {
    pub fn new(key: &Key) -> Self {
        // Sous-clés distinctes pour les empreintes et les fichiers (séparation des usages).
        Self {
            aead: aead(key),
            files: aead(&subkey(key, b"copyq-rs files v1")),
            fingerprint_key: subkey(key, b"copyq-rs fingerprint v1"),
        }
    }

    pub fn encrypt(&self, plain: &str) -> Result<String, String> {
        let nonce = Nonce::<Aes256Gcm>::generate();
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
        let blob = B64
            .decode(encoded)
            .map_err(|_| "base64 invalide".to_string())?;
        if blob.len() < NONCE_LEN + TAG_LEN {
            return Err("donnée tronquée".into());
        }
        let (nonce, ciphertext) = blob.split_at(NONCE_LEN);
        let plain = self
            .aead
            .decrypt(&nonce_from(nonce)?, ciphertext)
            .map_err(|_| "déchiffrement refusé (clé différente ou donnée altérée)".to_string())?;
        String::from_utf8(plain).map_err(|_| "texte non UTF-8".to_string())
    }

    /// Chiffre le contenu d'un fichier ; `name` (nom logique) est authentifié avec.
    pub fn encrypt_file(&self, plain: &[u8], name: &str) -> Result<Vec<u8>, String> {
        let nonce = Nonce::<Aes256Gcm>::generate();
        let ciphertext = self
            .files
            .encrypt(
                &nonce,
                Payload {
                    msg: plain,
                    aad: name.as_bytes(),
                },
            )
            .map_err(|_| "chiffrement impossible".to_string())?;
        let mut out = Vec::with_capacity(FILE_MAGIC.len() + NONCE_LEN + ciphertext.len());
        out.extend_from_slice(FILE_MAGIC);
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    pub fn decrypt_file(&self, data: &[u8], name: &str) -> Result<Vec<u8>, String> {
        let rest = data
            .strip_prefix(FILE_MAGIC)
            .ok_or("fichier non chiffré ou format inconnu")?;
        if rest.len() < NONCE_LEN + TAG_LEN {
            return Err("fichier tronqué".into());
        }
        let (nonce, ciphertext) = rest.split_at(NONCE_LEN);
        self.files
            .decrypt(
                &nonce_from(nonce)?,
                Payload {
                    msg: ciphertext,
                    aad: name.as_bytes(),
                },
            )
            .map_err(|_| "déchiffrement refusé (clé différente ou fichier altéré)".to_string())
    }

    pub fn fingerprint(&self, plain: &str) -> String {
        let mut mac =
            <Hmac<Sha256> as KeyInit>::new_from_slice(&self.fingerprint_key).expect("clé HMAC");
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

    #[test]
    fn files_roundtrip_and_are_bound_to_their_name() {
        let key = generate_key();
        let c = Cipher::new(&key);
        let png = b"\x89PNG\r\n\x1a\n fausse image".to_vec();
        let enc = c.encrypt_file(&png, "abc:full").unwrap();
        assert!(enc.starts_with(FILE_MAGIC));
        assert!(
            !enc.windows(4).any(|w| w == b"\x89PNG"),
            "pas d'en-tête PNG en clair"
        );
        assert_eq!(c.decrypt_file(&enc, "abc:full").unwrap(), png);
        assert!(c.decrypt_file(&enc, "abc:thumb").is_err(), "autre nom");
        assert!(Cipher::new(&generate_key())
            .decrypt_file(&enc, "abc:full")
            .is_err());
        assert!(
            c.decrypt_file(&png, "abc:full").is_err(),
            "fichier en clair"
        );
        // Sous-clé des fichiers ≠ clé des mots de passe.
        assert_ne!(subkey(&key, b"copyq-rs files v1"), key);
    }

    /// Les mots de passe chiffrés avant la mise à jour des bibliothèques restent lisibles.
    #[test]
    fn stored_format_is_stable() {
        let key = [7u8; 32];
        let c = Cipher::new(&key);
        // Valeurs calculées indépendamment (Python : hmac + cryptography).
        assert_eq!(
            c.fingerprint("abc"),
            "594c74244ec2a2cb9b997865aaf29586d9730bf1309ea57429b5eaade037140d"
        );
        assert_eq!(
            c.decrypt("enc:v1:AAECAwQFBgcICQoLde6dUHls+TkWwZb5wpvF15CHFlXkaidUoVLa1rMrHw==")
                .unwrap(),
            "mot de passe é"
        );
    }
}
