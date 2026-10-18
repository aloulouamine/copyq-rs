//! Images copiées : PNG + vignette, **chiffrés** sur le disque.
//!
//! La base ne garde qu'une référence (empreinte SHA-256 des pixels) : elle reste
//! légère et deux copies de la même image sont reconnues comme doublon, quel que
//! soit le format d'origine (PNG, TIFF…).
//!
//! Les fichiers (`<empreinte>.png.enc`, `<empreinte>.thumb.png.enc`) sont chiffrés en
//! AES-256-GCM avec une sous-clé de la clé du trousseau (voir `crypto`) : une capture
//! d'écran (relevé bancaire, code 2FA…) n'est jamais écrite en clair. Les images des
//! versions précédentes, en clair, sont chiffrées au démarrage (`migrate_plain`).

use crate::crypto::Cipher;
use image::{imageops::FilterType, ImageFormat, RgbaImage};
use sha2::{Digest, Sha256};
use std::io::Cursor;
use std::path::{Path, PathBuf};

/// Côté maximal de la vignette (pixels), assez grand pour l'aperçu au survol.
pub const THUMB_MAX: u32 = 360;
/// Par défaut, au-delà de 50 mégapixels l'image est ignorée (réglable).
pub const MAX_PIXELS: u64 = 50_000_000;

/// Image entière ou vignette.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Variant {
    Full,
    Thumb,
}

impl Variant {
    fn suffix(self) -> &'static str {
        match self {
            Variant::Full => "png",
            Variant::Thumb => "thumb.png",
        }
    }

    /// Nom logique authentifié avec le contenu chiffré (on ne peut pas échanger
    /// deux fichiers, ni une vignette et une image).
    fn aad(self, hash: &str) -> String {
        format!("copyq-rs image v1:{hash}:{}", self.suffix())
    }
}

pub fn images_dir() -> PathBuf {
    crate::history::data_dir().join("images")
}

pub fn path(dir: &Path, hash: &str, variant: Variant) -> PathBuf {
    dir.join(format!("{hash}.{}.enc", variant.suffix()))
}

pub fn full_path(dir: &Path, hash: &str) -> PathBuf {
    path(dir, hash, Variant::Full)
}

pub fn thumb_path(dir: &Path, hash: &str) -> PathBuf {
    path(dir, hash, Variant::Thumb)
}

/// Fichier en clair d'une version précédente.
fn legacy_path(dir: &Path, hash: &str, variant: Variant) -> PathBuf {
    dir.join(format!("{hash}.{}", variant.suffix()))
}

/// Résultat d'un enregistrement : référence et description (« 1280 × 720 · 245 Ko »).
pub struct Saved {
    pub hash: String,
    pub meta: String,
}

/// Empreinte des pixels (dimensions comprises).
pub fn hash_rgba(width: u32, height: u32, rgba: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(width.to_le_bytes());
    h.update(height.to_le_bytes());
    h.update(rgba);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Enregistre une image RGBA (PNG + vignette, chiffrés). Ne réécrit pas une image
/// déjà présente.
pub fn save_rgba(
    dir: &Path,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    cipher: &Cipher,
) -> Result<Saved, String> {
    save_rgba_limited(dir, width, height, rgba, MAX_PIXELS, cipher)
}

/// Vérifie, avant tout calcul coûteux, qu'une image peut être enregistrée.
pub fn check(width: u32, height: u32, rgba: &[u8], max_pixels: u64) -> Result<(), String> {
    if width == 0 || height == 0 {
        return Err("image vide".into());
    }
    let pixels = u64::from(width) * u64::from(height);
    if pixels > max_pixels {
        return Err(format!("image trop grande ({width} × {height})"));
    }
    if rgba.len() as u64 != pixels * 4 {
        return Err("données d'image incohérentes".into());
    }
    Ok(())
}

/// Dimensions affichées (« 1280 × 720 »), avant que la taille du fichier soit connue.
pub fn dimensions(width: u32, height: u32) -> String {
    format!("{width} × {height}")
}

/// Description complète (« 1280 × 720 · 245 Ko »).
pub fn describe(width: u32, height: u32, file_size: u64) -> String {
    format!("{} · {}", dimensions(width, height), human_size(file_size))
}

/// Taille du fichier d'une image déjà enregistrée.
pub fn stored_size(dir: &Path, hash: &str) -> Option<u64> {
    std::fs::metadata(full_path(dir, hash))
        .ok()
        .map(|m| m.len())
}

pub fn save_rgba_limited(
    dir: &Path,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    max_pixels: u64,
    cipher: &Cipher,
) -> Result<Saved, String> {
    check(width, height, &rgba, max_pixels)?;
    let hash = hash_rgba(width, height, &rgba);
    let size = write(dir, &hash, width, height, rgba, cipher)?;
    Ok(Saved {
        meta: describe(width, height, size),
        hash,
    })
}

/// Écrit l'image et sa vignette (la partie lente : encodage PNG, puis chiffrement) ;
/// renvoie la taille du fichier. Les fichiers déjà présents ne sont pas réécrits.
pub fn write(
    dir: &Path,
    hash: &str,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    cipher: &Cipher,
) -> Result<u64, String> {
    let full = full_path(dir, hash);
    let img = RgbaImage::from_raw(width, height, rgba).ok_or("données d'image incohérentes")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    if !full.exists() {
        write_encrypted(&full, &encode_png(&img)?, cipher, &Variant::Full.aad(hash))?;
    }
    let thumb = thumb_path(dir, hash);
    if !thumb.exists() {
        let small = if width.max(height) > THUMB_MAX {
            image::imageops::resize(
                &img,
                (width * THUMB_MAX / width.max(height)).max(1),
                (height * THUMB_MAX / width.max(height)).max(1),
                FilterType::Triangle,
            )
        } else {
            img
        };
        write_encrypted(
            &thumb,
            &encode_png(&small)?,
            cipher,
            &Variant::Thumb.aad(hash),
        )?;
    }
    Ok(std::fs::metadata(&full).map(|m| m.len()).unwrap_or(0))
}

fn encode_png(img: &RgbaImage) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    img.write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .map_err(|e| format!("encodage PNG : {e}"))?;
    Ok(bytes)
}

/// Chiffre puis écrit de façon atomique (fichier temporaire, puis renommage).
fn write_encrypted(path: &Path, plain: &[u8], cipher: &Cipher, aad: &str) -> Result<(), String> {
    let data = cipher.encrypt_file(plain, aad)?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// Pixels RGBA d'une image enregistrée (vignette à afficher, image à recopier).
pub fn load_rgba(
    dir: &Path,
    hash: &str,
    variant: Variant,
    cipher: &Cipher,
) -> Result<(u32, u32, Vec<u8>), String> {
    let path = path(dir, hash, variant);
    let data = std::fs::read(&path).map_err(|e| format!("{} : {e}", path.display()))?;
    let png = cipher.decrypt_file(&data, &variant.aad(hash))?;
    let img = image::load_from_memory_with_format(&png, ImageFormat::Png)
        .map_err(|e| format!("{} : {e}", path.display()))?;
    let rgba = img.to_rgba8();
    Ok((rgba.width(), rgba.height(), rgba.into_raw()))
}

/// Vrai s'il reste des images en clair d'une version précédente (sans accès au trousseau).
pub fn has_plain(dir: &Path) -> bool {
    std::fs::read_dir(dir).is_ok_and(|entries| {
        entries
            .flatten()
            .any(|e| e.path().extension().is_some_and(|x| x == "png"))
    })
}

/// Chiffre les images restées en clair (versions précédentes), puis supprime les
/// originaux. Renvoie le nombre de fichiers chiffrés.
pub fn migrate_plain(dir: &Path, cipher: &Cipher) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut done = 0;
    for entry in entries.flatten() {
        let plain = entry.path();
        let Some(name) = plain.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let (hash, variant) = if let Some(h) = name.strip_suffix(".thumb.png") {
            (h, Variant::Thumb)
        } else if let Some(h) = name.strip_suffix(".png") {
            (h, Variant::Full)
        } else {
            continue;
        };
        let target = path(dir, hash, variant);
        let result = if target.exists() {
            Ok(())
        } else {
            std::fs::read(&plain)
                .map_err(|e| e.to_string())
                .and_then(|bytes| write_encrypted(&target, &bytes, cipher, &variant.aad(hash)))
        };
        match result {
            Ok(()) => {
                remove_file_logged(&plain);
                done += 1;
            }
            Err(e) => log::warn!("{name} non chiffrée : {e}"),
        }
    }
    done
}

/// Supprime un fichier ; l'absence n'est pas une erreur, le reste est journalisé.
fn remove_file_logged(path: &Path) {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => log::warn!("suppression de {} impossible : {e}", path.display()),
    }
}

/// Supprime l'image et sa vignette (chiffrées, et en clair d'une ancienne version).
pub fn remove(dir: &Path, hash: &str) {
    for variant in [Variant::Full, Variant::Thumb] {
        remove_file_logged(&path(dir, hash, variant));
        remove_file_logged(&legacy_path(dir, hash, variant));
    }
}

/// Supprime toutes les images.
pub fn remove_all(dir: &Path) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            if e.path()
                .extension()
                .is_some_and(|x| x == "png" || x == "enc" || x == "tmp")
            {
                remove_file_logged(&e.path());
            }
        }
    }
}

pub fn human_size(bytes: u64) -> String {
    match bytes {
        0..=999 => format!("{bytes} o"),
        1_000..=999_999 => format!("{} Ko", bytes / 1_000),
        _ => format!("{:.1} Mo", bytes as f64 / 1_000_000.0).replace('.', ","),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::generate_key;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("copyq-rs-media-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn checker(w: u32, h: u32) -> Vec<u8> {
        (0..w * h)
            .flat_map(|i| {
                if (i % w + i / w).is_multiple_of(2) {
                    [255, 0, 0, 255]
                } else {
                    [0, 0, 255, 255]
                }
            })
            .collect()
    }

    fn is_png(path: &Path) -> bool {
        std::fs::read(path).is_ok_and(|b| b.starts_with(b"\x89PNG"))
    }

    #[test]
    fn saves_encrypted_png_and_thumbnail_and_reloads_pixels() {
        let dir = temp("save");
        let c = Cipher::new(&generate_key());
        let rgba = checker(800, 400);
        let saved = save_rgba(&dir, 800, 400, rgba.clone(), &c).unwrap();
        assert!(saved.meta.starts_with("800 × 400 · "), "{}", saved.meta);
        let full = full_path(&dir, &saved.hash);
        assert!(full.exists());
        assert!(!is_png(&full), "jamais d'image en clair sur le disque");
        assert!(!is_png(&thumb_path(&dir, &saved.hash)));
        assert!(!legacy_path(&dir, &saved.hash, Variant::Full).exists());
        let (tw, th, _) = load_rgba(&dir, &saved.hash, Variant::Thumb, &c).unwrap();
        assert_eq!((tw, th), (THUMB_MAX, THUMB_MAX / 2));
        let (w, h, px) = load_rgba(&dir, &saved.hash, Variant::Full, &c).unwrap();
        assert_eq!((w, h), (800, 400));
        assert_eq!(px, rgba, "PNG sans perte");
        // Autre clé : illisible.
        let other = Cipher::new(&generate_key());
        assert!(load_rgba(&dir, &saved.hash, Variant::Full, &other).is_err());

        // Même image : même empreinte, pas de nouveau fichier.
        let again = save_rgba(&dir, 800, 400, rgba, &c).unwrap();
        assert_eq!(again.hash, saved.hash);
        remove(&dir, &saved.hash);
        assert!(!full_path(&dir, &saved.hash).exists());
        assert!(!thumb_path(&dir, &saved.hash).exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn plain_images_from_previous_versions_are_encrypted() {
        let dir = temp("migrate");
        std::fs::create_dir_all(&dir).unwrap();
        let c = Cipher::new(&generate_key());
        let img = RgbaImage::from_raw(4, 2, checker(4, 2)).unwrap();
        let png = encode_png(&img).unwrap();
        std::fs::write(legacy_path(&dir, "abc", Variant::Full), &png).unwrap();
        std::fs::write(legacy_path(&dir, "abc", Variant::Thumb), &png).unwrap();
        assert!(has_plain(&dir));
        assert_eq!(migrate_plain(&dir, &c), 2);
        assert!(!has_plain(&dir), "originaux en clair supprimés");
        assert!(!is_png(&full_path(&dir, "abc")));
        let (w, h, _) = load_rgba(&dir, "abc", Variant::Full, &c).unwrap();
        assert_eq!((w, h), (4, 2));
        assert!(load_rgba(&dir, "abc", Variant::Thumb, &c).is_ok());
        assert_eq!(migrate_plain(&dir, &c), 0, "rien de plus à faire");
        remove_all(&dir);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn rejects_empty_and_huge_images() {
        let dir = temp("reject");
        let c = Cipher::new(&generate_key());
        assert!(save_rgba(&dir, 0, 10, vec![], &c).is_err());
        assert!(save_rgba(&dir, 100_000, 100_000, vec![], &c).is_err());
        assert!(
            save_rgba(&dir, 2, 2, vec![0; 3], &c).is_err(),
            "taille incohérente"
        );
    }

    #[test]
    fn sizes_are_human_readable() {
        assert_eq!(human_size(512), "512 o");
        assert_eq!(human_size(245_000), "245 Ko");
        assert_eq!(human_size(3_400_000), "3,4 Mo");
    }
}
