//! Images copiées : enregistrement en PNG (avec vignette) dans un dossier dédié.
//!
//! La base ne garde qu'une référence (empreinte SHA-256 des pixels) : elle reste
//! légère et deux copies de la même image sont reconnues comme doublon, quel que
//! soit le format d'origine (PNG, TIFF…).

use image::{imageops::FilterType, ImageFormat, RgbaImage};
use sha2::{Digest, Sha256};
use std::io::Cursor;
use std::path::{Path, PathBuf};

/// Côté maximal de la vignette (pixels), assez grand pour l'aperçu au survol.
pub const THUMB_MAX: u32 = 360;
/// Par défaut, au-delà de 50 mégapixels l'image est ignorée (réglable).
pub const MAX_PIXELS: u64 = 50_000_000;

pub fn images_dir() -> PathBuf {
    crate::history::data_dir().join("images")
}

pub fn full_path(dir: &Path, hash: &str) -> PathBuf {
    dir.join(format!("{hash}.png"))
}

pub fn thumb_path(dir: &Path, hash: &str) -> PathBuf {
    dir.join(format!("{hash}.thumb.png"))
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

/// Enregistre une image RGBA en PNG + vignette. Ne réécrit pas une image déjà présente.
pub fn save_rgba(dir: &Path, width: u32, height: u32, rgba: Vec<u8>) -> Result<Saved, String> {
    save_rgba_limited(dir, width, height, rgba, MAX_PIXELS)
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

/// Taille du fichier PNG d'une image déjà enregistrée.
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
) -> Result<Saved, String> {
    check(width, height, &rgba, max_pixels)?;
    let hash = hash_rgba(width, height, &rgba);
    let size = write(dir, &hash, width, height, rgba)?;
    Ok(Saved {
        meta: describe(width, height, size),
        hash,
    })
}

/// Écrit le PNG et la vignette (la partie lente : encodage) ; renvoie la taille du PNG.
/// Les fichiers déjà présents ne sont pas réécrits.
pub fn write(
    dir: &Path,
    hash: &str,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
) -> Result<u64, String> {
    let full = full_path(dir, hash);
    let img = RgbaImage::from_raw(width, height, rgba).ok_or("données d'image incohérentes")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    if !full.exists() {
        write_png_atomic(&full, &img)?;
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
            img.clone()
        };
        write_png_atomic(&thumb, &small)?;
    }
    Ok(std::fs::metadata(&full).map(|m| m.len()).unwrap_or(0))
}

fn write_png_atomic(path: &Path, img: &RgbaImage) -> Result<(), String> {
    let mut bytes = Vec::new();
    img.write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .map_err(|e| format!("encodage PNG : {e}"))?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// Pixels RGBA d'une image enregistrée (pour la remettre dans le presse-papiers).
pub fn load_rgba(path: &Path) -> Result<(u32, u32, Vec<u8>), String> {
    let img = image::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let rgba = img.to_rgba8();
    Ok((rgba.width(), rgba.height(), rgba.into_raw()))
}

/// Supprime un fichier ; l'absence n'est pas une erreur, le reste est journalisé.
fn remove_file_logged(path: &Path) {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => log::warn!("suppression de {} impossible : {e}", path.display()),
    }
}

/// Supprime l'image et sa vignette.
pub fn remove(dir: &Path, hash: &str) {
    remove_file_logged(&full_path(dir, hash));
    remove_file_logged(&thumb_path(dir, hash));
}

/// Supprime toutes les images.
pub fn remove_all(dir: &Path) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            if e.path().extension().is_some_and(|x| x == "png") {
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

    #[test]
    fn saves_png_and_thumbnail_and_reloads_pixels() {
        let dir = temp("save");
        let rgba = checker(800, 400);
        let saved = save_rgba(&dir, 800, 400, rgba.clone()).unwrap();
        assert!(saved.meta.starts_with("800 × 400 · "), "{}", saved.meta);
        assert!(full_path(&dir, &saved.hash).exists());
        let (tw, th, _) = load_rgba(&thumb_path(&dir, &saved.hash)).unwrap();
        assert_eq!((tw, th), (THUMB_MAX, THUMB_MAX / 2));
        let (w, h, px) = load_rgba(&full_path(&dir, &saved.hash)).unwrap();
        assert_eq!((w, h), (800, 400));
        assert_eq!(px, rgba, "PNG sans perte");

        // Même image : même empreinte, pas de nouveau fichier.
        let again = save_rgba(&dir, 800, 400, rgba).unwrap();
        assert_eq!(again.hash, saved.hash);
        remove(&dir, &saved.hash);
        assert!(!full_path(&dir, &saved.hash).exists());
        assert!(!thumb_path(&dir, &saved.hash).exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn rejects_empty_and_huge_images() {
        let dir = temp("reject");
        assert!(save_rgba(&dir, 0, 10, vec![]).is_err());
        assert!(save_rgba(&dir, 100_000, 100_000, vec![]).is_err());
        assert!(
            save_rgba(&dir, 2, 2, vec![0; 3]).is_err(),
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
