//! Surveillance du presse-papiers système dans un thread dédié.
//!
//! macOS ne notifie pas les changements : on consulte toutes les 300 ms le
//! *compteur de changements* du presse-papiers (opération quasi gratuite) et on
//! ne lit le contenu que lorsqu'il a bougé. Les copies marquées confidentielles
//! par les gestionnaires de mots de passe sont ignorées sans être lues.

use crate::crypto::Cipher;
use crate::history::{History, Item, ItemKind};
use crate::keychain;
use crate::logging::LockExt;
use crate::media::{self, Variant};
use crate::pasteboard::{self, Payload};
use eframe::egui;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

const POLL_INTERVAL: Duration = Duration::from_millis(300);

/// Ce que le surveillant doit faire après un tour de boucle.
#[derive(Debug, PartialEq)]
enum Change {
    /// Rien de nouveau.
    None,
    /// Nouveau contenu, mais à ignorer (pause ou contenu confidentiel).
    Skip(&'static str),
    /// Nouveau contenu à lire et enregistrer ; `sensitive` = mot de passe
    /// (enregistré seulement si l'utilisateur l'a autorisé, affiché masqué).
    Read { sensitive: bool },
}

/// Décide quoi faire à partir du compteur (None si indisponible hors macOS).
fn classify(
    last_count: &mut Option<isize>,
    count: Option<isize>,
    paused: bool,
    save_passwords: bool,
    types: impl FnOnce() -> Vec<String>,
) -> Change {
    let previous = *last_count;
    if let Some(c) = count {
        if previous == Some(c) {
            return Change::None;
        }
        *last_count = Some(c);
    }
    if paused {
        return Change::Skip("pause");
    }
    let types = types();
    if count.is_some() && types.is_empty() {
        // macOS incrémente le compteur quand une app *vide* le presse-papiers, avant
        // qu'elle n'y écrive (sans nouvel incrément). On ne mémorise donc pas ce
        // compteur : on réessaiera au prochain tour, une fois le contenu écrit.
        *last_count = previous;
        return Change::None;
    }
    let sensitive = pasteboard::should_ignore(&types);
    if sensitive && !save_passwords {
        return Change::Skip("contenu confidentiel");
    }
    Change::Read { sensitive }
}

/// Réglages et états partagés entre l'interface et la surveillance.
#[derive(Clone)]
pub struct Shared {
    pub paused: Arc<AtomicBool>,
    pub save_passwords: Arc<AtomicBool>,
    pub detect_passwords: Arc<AtomicBool>,
    pub save_images: Arc<AtomicBool>,
    /// Compteur du presse-papiers après une écriture de l'app elle-même (recopie
    /// d'un élément) : cette « copie » ne doit pas être réenregistrée.
    pub own_change: Arc<AtomicIsize>,
    /// Taille maximale d'une image (pixels).
    pub max_image_pixels: Arc<AtomicU64>,
    /// Nombre maximal d'images gardées : à 0, les images ne sont même pas lues.
    pub max_images: Arc<AtomicUsize>,
}

impl Shared {
    pub fn new(save_passwords: bool, detect_passwords: bool, save_images: bool) -> Self {
        Self {
            paused: Arc::new(AtomicBool::new(false)),
            save_passwords: Arc::new(AtomicBool::new(save_passwords)),
            detect_passwords: Arc::new(AtomicBool::new(detect_passwords)),
            save_images: Arc::new(AtomicBool::new(save_images)),
            own_change: Arc::new(AtomicIsize::new(isize::MIN)),
            max_image_pixels: Arc::new(AtomicU64::new(media::MAX_PIXELS)),
            max_images: Arc::new(AtomicUsize::new(crate::history::MAX_IMAGES)),
        }
    }

    /// À appeler juste après que l'app a écrit dans le presse-papiers.
    pub fn mark_own_write(&self) {
        if let Some(c) = pasteboard::change_count() {
            self.own_change.store(c, Ordering::Relaxed);
        }
    }
}

/// Faut-il lire et enregistrer les images copiées ?
fn images_wanted(shared: &Shared) -> bool {
    shared.save_images.load(Ordering::Relaxed) && shared.max_images.load(Ordering::Relaxed) > 0
}

/// Image à encoder (PNG + vignette) hors du fil de surveillance.
struct ImageJob {
    hash: String,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

/// Fil dédié à l'écriture des images. File d'attente courte : au-delà, la
/// surveillance attend (pas d'accumulation de captures en mémoire).
struct Encoder {
    tx: Option<mpsc::SyncSender<ImageJob>>,
    history: Arc<Mutex<History>>,
    dir: PathBuf,
    ctx: egui::Context,
    /// Chiffreur des fichiers (clé du trousseau, demandée au premier besoin).
    cipher: CipherSource,
}

/// Fournit le chiffreur (le trousseau en production, une clé de test sinon).
type CipherSource = fn() -> Result<Arc<Cipher>, String>;

impl Encoder {
    fn spawn(history: Arc<Mutex<History>>, dir: PathBuf, ctx: egui::Context) -> Self {
        let (tx, rx) = mpsc::sync_channel::<ImageJob>(2);
        let cipher: CipherSource = keychain::shared_cipher;
        let worker = Encoder {
            tx: None,
            history: history.clone(),
            dir: dir.clone(),
            ctx: ctx.clone(),
            cipher,
        };
        let tx = thread::Builder::new()
            .name("image-encoder".into())
            .spawn(move || {
                worker.migrate_plain_images();
                for job in rx {
                    worker.encode(job);
                }
            })
            .map_err(|e| log::error!("fil des images indisponible ({e}) : encodage sur place"))
            .ok()
            .map(|_| tx);
        Encoder {
            tx,
            history,
            dir,
            ctx,
            cipher,
        }
    }

    /// Images en clair d'une version précédente : chiffrées une fois pour toutes.
    /// Le trousseau n'est sollicité que s'il y en a.
    fn migrate_plain_images(&self) {
        if !media::has_plain(&self.dir) {
            return;
        }
        match (self.cipher)() {
            Ok(c) => {
                let n = media::migrate_plain(&self.dir, &c);
                log::info!("{n} fichier(s) image d'une version précédente désormais chiffré(s)");
            }
            Err(e) => log::error!("images en clair non chiffrées (trousseau : {e})"),
        }
        self.ctx.request_repaint();
    }

    fn submit(&self, job: ImageJob) {
        match &self.tx {
            Some(tx) => {
                if let Err(mpsc::SendError(job)) = tx.send(job) {
                    log::error!("fil des images arrêté : encodage sur place");
                    self.encode(job);
                }
            }
            None => self.encode(job),
        }
    }

    fn encode(&self, job: ImageJob) {
        let ImageJob {
            hash,
            width,
            height,
            rgba,
        } = job;
        // Jamais d'image en clair : sans clé, l'image n'est pas gardée.
        let written = (self.cipher)()
            .map_err(|e| format!("trousseau inaccessible ({e})"))
            .and_then(|c| media::write(&self.dir, &hash, width, height, rgba, &c));
        match written {
            Ok(size) => {
                let mut h = self.history.lock_safe();
                if h.has_image(&hash) {
                    h.set_image_meta(&hash, media::describe(width, height, size));
                } else {
                    // Supprimée (ou évincée) pendant l'encodage : pas de fichier orphelin.
                    drop(h);
                    media::remove(&self.dir, &hash);
                }
            }
            Err(e) => {
                log::warn!("image non enregistrée : {e}");
                self.history.lock_safe().remove_image(&hash);
            }
        }
        self.ctx.request_repaint();
    }
}

pub fn spawn_watcher(history: Arc<Mutex<History>>, shared: Shared, ctx: egui::Context) {
    let images_dir = media::images_dir();
    let encoder = Encoder::spawn(history.clone(), images_dir.clone(), ctx.clone());
    let spawned = thread::Builder::new()
        .name("clipboard-watcher".into())
        .spawn(move || {
            let mut clipboard = match arboard::Clipboard::new() {
                Ok(c) => c,
                Err(e) => {
                    log::error!("Impossible d'accéder au presse-papiers : {e}");
                    return;
                }
            };
            // Au démarrage on part du compteur actuel : ce qui est déjà dans le
            // presse-papiers n'est pas « nouveau ».
            let mut last_count = pasteboard::change_count();
            // Hors macOS (pas de compteur), on compare le texte.
            let mut last_text: Option<String> = None;
            loop {
                thread::sleep(POLL_INTERVAL);
                let count = pasteboard::change_count();
                if let Some(c) = count {
                    if c == shared.own_change.load(Ordering::Relaxed) && last_count != Some(c) {
                        last_count = Some(c); // recopie faite par l'app elle-même
                        continue;
                    }
                }
                let mut seen_types = Vec::new();
                let mut pending_image: Option<ImageJob> = None;
                let change = classify(
                    &mut last_count,
                    count,
                    shared.paused.load(Ordering::Relaxed),
                    shared.save_passwords.load(Ordering::Relaxed),
                    || {
                        let types = pasteboard::types();
                        // Seulement les identifiants de types, jamais le contenu.
                        log::info!("nouvelle copie, types : {types:?}");
                        seen_types = types.clone();
                        types
                    },
                );
                let sensitive = match change {
                    Change::None => continue,
                    Change::Skip(reason) => {
                        log::info!("Copie ignorée ({reason})");
                        continue;
                    }
                    Change::Read { sensitive } => sensitive,
                };

                let item = match pasteboard::classify_payload(&seen_types) {
                    Payload::Files if !sensitive => {
                        let paths = pasteboard::file_paths();
                        if paths.is_empty() {
                            continue;
                        }
                        log::info!("{} fichier(s) copié(s)", paths.len());
                        Item {
                            text: paths
                                .iter()
                                .map(|p| p.to_string_lossy())
                                .collect::<Vec<_>>()
                                .join("\n"),
                            kind: ItemKind::Files,
                            ..Default::default()
                        }
                    }
                    Payload::Image if !sensitive => {
                        if !images_wanted(&shared) {
                            log::info!("Copie ignorée (image : désactivé ou limite à 0)");
                            continue;
                        }
                        let Ok(img) = clipboard.get_image() else {
                            continue;
                        };
                        let (w, h) = (img.width as u32, img.height as u32);
                        let rgba = img.bytes.into_owned();
                        let max = shared.max_image_pixels.load(Ordering::Relaxed);
                        if let Err(e) = media::check(w, h, &rgba, max) {
                            log::warn!("image non enregistrée : {e}");
                            continue;
                        }
                        // L'empreinte est rapide ; l'encodage PNG (lent pour une capture
                        // 5K) est confié au fil des images, pour ne pas retarder les copies
                        // suivantes. L'élément apparaît tout de suite, à sa place.
                        let hash = media::hash_rgba(w, h, &rgba);
                        let item = Item {
                            text: hash.clone(),
                            kind: ItemKind::Image,
                            meta: match media::stored_size(&images_dir, &hash) {
                                Some(size) => media::describe(w, h, size),
                                None => {
                                    pending_image = Some(ImageJob {
                                        hash,
                                        width: w,
                                        height: h,
                                        rgba,
                                    });
                                    media::dimensions(w, h)
                                }
                            },
                            ..Default::default()
                        };
                        log::info!("image copiée : {}", item.meta);
                        item
                    }
                    _ => {
                        // Erreur = contenu non textuel inconnu : ignoré.
                        let Ok(text) = clipboard.get_text() else {
                            continue;
                        };
                        if count.is_none() {
                            if last_text.as_deref() == Some(text.as_str()) {
                                continue;
                            }
                            last_text = Some(text.clone());
                        }
                        // Mot de passe copié sans marqueur (extension de navigateur…) ?
                        let probable = !sensitive
                            && shared.detect_passwords.load(Ordering::Relaxed)
                            && pasteboard::looks_like_password(&text);
                        if probable && !shared.save_passwords.load(Ordering::Relaxed) {
                            log::info!("Copie ignorée (mot de passe probable)");
                            continue;
                        }
                        let sensitive = sensitive || probable;
                        if sensitive {
                            log::info!(
                                "Mot de passe enregistré ({}), chiffré et affiché masqué",
                                if probable {
                                    "détecté"
                                } else {
                                    "marqué confidentiel"
                                }
                            );
                        }
                        Item::text(text, 0, sensitive)
                    }
                };
                if history.lock_safe().push_item(item) {
                    ctx.request_repaint();
                }
                // Après l'ajout : le fil des images vérifie que l'image est toujours là.
                if let Some(job) = pending_image.take() {
                    encoder.submit(job);
                }
            }
        });
    if let Err(e) = spawned {
        log::error!("Impossible de lancer la surveillance du presse-papiers : {e}");
    }
}

/// Remet un élément dans le presse-papiers (texte, image ou fichiers).
pub fn write_item(clipboard: &mut arboard::Clipboard, item: &Item) -> Result<(), String> {
    match item.kind {
        ItemKind::Text => clipboard
            .set_text(item.text.clone())
            .map_err(|e| e.to_string()),
        ItemKind::Image => {
            let cipher = keychain::shared_cipher()?;
            let (w, h, rgba) =
                media::load_rgba(&media::images_dir(), &item.text, Variant::Full, &cipher)
                    .map_err(|e| {
                        log::warn!("image illisible : {e}");
                        "image introuvable ou illisible".to_string()
                    })?;
            clipboard
                .set_image(arboard::ImageData {
                    width: w as usize,
                    height: h as usize,
                    bytes: rgba.into(),
                })
                .map_err(|e| e.to_string())
        }
        ItemKind::Files => {
            let existing: Vec<_> = item.paths().into_iter().filter(|p| p.exists()).collect();
            if existing.is_empty() {
                return Err("fichier(s) introuvable(s) — déplacé(s) ou supprimé(s) ?".into());
            }
            if pasteboard::write_file_paths(&existing) {
                Ok(())
            } else {
                Err("écriture des fichiers dans le presse-papiers impossible".into())
            }
        }
    }
}

/// Auto-test CI : `copyq-rs --media-selftest` (écrit puis relit image et fichiers).
pub fn media_selftest() -> i32 {
    let mut ok = true;
    let mut cb = match arboard::Clipboard::new() {
        Ok(c) => c,
        Err(e) => {
            println!("ÉCHEC presse-papiers : {e}");
            return 1;
        }
    };
    let dir = std::env::temp_dir().join(format!("copyq-rs-media-selftest-{}", std::process::id()));
    let px: Vec<u8> = (0..64 * 32)
        .flat_map(|i| [(i % 256) as u8, 80, 160, 255])
        .collect();
    // Clé jetable : l'auto-test ne touche pas au trousseau.
    let cipher = Cipher::new(&crate::crypto::generate_key());
    let saved = media::save_rgba(&dir, 64, 32, px, &cipher).expect("image de test");
    println!("image de test enregistrée : {}", saved.meta);

    // Recopie d'image : on relit (et déchiffre) le fichier enregistré.
    let path = media::full_path(&dir, &saved.hash);
    let (w, h, rgba) = media::load_rgba(&dir, &saved.hash, Variant::Full, &cipher).unwrap();
    let wrote = cb
        .set_image(arboard::ImageData {
            width: w as usize,
            height: h as usize,
            bytes: rgba.into(),
        })
        .is_ok();
    let types = pasteboard::types();
    let payload = pasteboard::classify_payload(&types);
    let back = cb.get_image().map(|i| (i.width, i.height)).ok();
    println!("image : écrite={wrote} types={types:?} → {payload:?}, relue {back:?}");
    ok &= wrote && payload == Payload::Image && back == Some((64, 32));

    // Recopie de fichiers.
    let files = vec![path.clone(), media::thumb_path(&dir, &saved.hash)];
    let wrote = pasteboard::write_file_paths(&files);
    let types = pasteboard::types();
    let back = pasteboard::file_paths();
    println!(
        "fichiers : écrits={wrote} → {:?}, relus {back:?}",
        pasteboard::classify_payload(&types)
    );
    let canon = |v: &[std::path::PathBuf]| -> Vec<_> {
        v.iter().filter_map(|p| p.canonicalize().ok()).collect()
    };
    ok &= wrote
        && pasteboard::classify_payload(&types) == Payload::Files
        && canon(&back) == canon(&files);

    let _ = std::fs::remove_dir_all(dir);
    println!("{}", if ok { "SUCCÈS" } else { "ÉCHEC" });
    i32::from(!ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoder_writes_files_then_completes_meta_and_leaves_no_orphans() {
        let dir = std::env::temp_dir().join(format!("copyq-rs-encoder-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let history = Arc::new(Mutex::new(History::default()));
        fn test_cipher() -> Result<Arc<Cipher>, String> {
            static C: std::sync::OnceLock<Arc<Cipher>> = std::sync::OnceLock::new();
            Ok(
                C.get_or_init(|| Arc::new(Cipher::new(&crate::crypto::generate_key())))
                    .clone(),
            )
        }
        let enc = Encoder {
            tx: None,
            history: history.clone(),
            dir: dir.clone(),
            ctx: egui::Context::default(),
            cipher: test_cipher,
        };
        let job = |seed: u8| {
            let rgba = vec![seed; 8 * 4 * 4];
            ImageJob {
                hash: media::hash_rgba(8, 4, &rgba),
                width: 8,
                height: 4,
                rgba,
            }
        };
        // Image toujours dans l'historique : fichiers écrits, taille ajoutée.
        let kept = job(1);
        let hash = kept.hash.clone();
        history.lock_safe().push_item(Item {
            text: hash.clone(),
            kind: ItemKind::Image,
            meta: media::dimensions(8, 4),
            ..Default::default()
        });
        enc.encode(kept);
        assert!(media::full_path(&dir, &hash).exists());
        assert!(media::thumb_path(&dir, &hash).exists());
        let meta = history.lock_safe().items()[0].meta.clone();
        assert!(meta.starts_with("8 × 4 · "), "{meta}");
        // Image supprimée pendant l'encodage : aucun fichier ne reste.
        let gone = job(2);
        let gone_hash = gone.hash.clone();
        enc.encode(gone);
        assert!(!media::full_path(&dir, &gone_hash).exists());
        assert!(!media::thumb_path(&dir, &gone_hash).exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn images_are_skipped_when_disabled_or_limit_is_zero() {
        let shared = Shared::new(false, true, true);
        assert!(images_wanted(&shared));
        shared.max_images.store(0, Ordering::Relaxed);
        assert!(
            !images_wanted(&shared),
            "limite à 0 : l'image n'est ni lue ni encodée"
        );
        shared.max_images.store(5, Ordering::Relaxed);
        shared.save_images.store(false, Ordering::Relaxed);
        assert!(!images_wanted(&shared));
    }

    fn plain() -> Vec<String> {
        vec!["public.utf8-plain-text".into()]
    }

    #[test]
    fn unchanged_counter_does_nothing() {
        let mut last = Some(5);
        assert_eq!(
            classify(&mut last, Some(5), false, false, plain),
            Change::None
        );
    }

    #[test]
    fn new_counter_reads() {
        let mut last = Some(5);
        assert_eq!(
            classify(&mut last, Some(6), false, false, plain),
            Change::Read { sensitive: false }
        );
        assert_eq!(last, Some(6));
        assert_eq!(
            classify(&mut last, Some(6), false, false, plain),
            Change::None
        );
    }

    #[test]
    fn concealed_is_skipped_and_not_reread() {
        let mut last = Some(1);
        let secret = || vec!["org.nspasteboard.ConcealedType".to_string()];
        assert!(matches!(
            classify(&mut last, Some(2), false, false, secret),
            Change::Skip(_)
        ));
        // Le compteur a été mémorisé : on ne retente pas sur le même contenu.
        assert_eq!(
            classify(&mut last, Some(2), false, false, plain),
            Change::None
        );
    }

    #[test]
    fn pause_skips_without_reading_types() {
        let mut last = Some(1);
        let r = classify(&mut last, Some(2), true, false, || {
            panic!("types lus pendant la pause")
        });
        assert_eq!(r, Change::Skip("pause"));
    }

    #[test]
    fn cleared_but_not_yet_written_is_retried() {
        let mut last = Some(1);
        // L'app source vient de vider le presse-papiers : rien à lire encore.
        assert_eq!(
            classify(&mut last, Some(2), false, false, Vec::new),
            Change::None
        );
        assert_eq!(last, Some(1), "le compteur ne doit pas être mémorisé");
        // Le contenu est écrit (même compteur) : on le lit bien.
        assert_eq!(
            classify(&mut last, Some(2), false, false, plain),
            Change::Read { sensitive: false }
        );
        assert_eq!(last, Some(2));
    }

    #[test]
    fn concealed_is_kept_when_allowed_and_marked_sensitive() {
        let mut last = Some(1);
        let secret = || vec!["org.nspasteboard.ConcealedType".to_string()];
        assert_eq!(
            classify(&mut last, Some(2), false, true, secret),
            Change::Read { sensitive: true }
        );
    }

    #[test]
    fn without_counter_always_reads() {
        let mut last = None;
        assert_eq!(
            classify(&mut last, None, false, false, plain),
            Change::Read { sensitive: false }
        );
    }
}
