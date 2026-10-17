//! Surveillance du presse-papiers système dans un thread dédié.
//!
//! macOS ne notifie pas les changements : on consulte toutes les 300 ms le
//! *compteur de changements* du presse-papiers (opération quasi gratuite) et on
//! ne lit le contenu que lorsqu'il a bougé. Les copies marquées confidentielles
//! par les gestionnaires de mots de passe sont ignorées sans être lues.

use crate::history::History;
use crate::logging::LockExt;
use crate::pasteboard;
use eframe::egui;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
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
    /// Nouveau contenu à lire et enregistrer.
    Read,
}

/// Décide quoi faire à partir du compteur (None si indisponible hors macOS).
fn classify(
    last_count: &mut Option<isize>,
    count: Option<isize>,
    paused: bool,
    types: impl FnOnce() -> Vec<String>,
) -> Change {
    if let Some(c) = count {
        if *last_count == Some(c) {
            return Change::None;
        }
        *last_count = Some(c);
    }
    if paused {
        return Change::Skip("pause");
    }
    if pasteboard::should_ignore(&types()) {
        return Change::Skip("contenu confidentiel");
    }
    Change::Read
}

pub fn spawn_watcher(history: Arc<Mutex<History>>, paused: Arc<AtomicBool>, ctx: egui::Context) {
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
                match classify(
                    &mut last_count,
                    count,
                    paused.load(Ordering::Relaxed),
                    pasteboard::types,
                ) {
                    Change::None => continue,
                    Change::Skip(reason) => {
                        log::info!("Copie ignorée ({reason})");
                        continue;
                    }
                    Change::Read => {}
                }
                // Erreur = contenu non textuel (image, fichier…) : ignoré.
                let Ok(text) = clipboard.get_text() else { continue };
                if count.is_none() {
                    if last_text.as_deref() == Some(text.as_str()) {
                        continue;
                    }
                    last_text = Some(text.clone());
                }
                let changed = {
                    let mut h = history.lock_safe();
                    let changed = h.push(text);
                    if changed {
                        if let Err(e) = h.save() {
                            log::error!("Échec de la sauvegarde : {e}");
                        }
                    }
                    changed
                };
                if changed {
                    ctx.request_repaint();
                }
            }
        });
    if let Err(e) = spawned {
        log::error!("Impossible de lancer la surveillance du presse-papiers : {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain() -> Vec<String> {
        vec!["public.utf8-plain-text".into()]
    }

    #[test]
    fn unchanged_counter_does_nothing() {
        let mut last = Some(5);
        assert_eq!(classify(&mut last, Some(5), false, plain), Change::None);
    }

    #[test]
    fn new_counter_reads() {
        let mut last = Some(5);
        assert_eq!(classify(&mut last, Some(6), false, plain), Change::Read);
        assert_eq!(last, Some(6));
        assert_eq!(classify(&mut last, Some(6), false, plain), Change::None);
    }

    #[test]
    fn concealed_is_skipped_and_not_reread() {
        let mut last = Some(1);
        let secret = || vec!["org.nspasteboard.ConcealedType".to_string()];
        assert!(matches!(classify(&mut last, Some(2), false, secret), Change::Skip(_)));
        // Le compteur a été mémorisé : on ne retente pas sur le même contenu.
        assert_eq!(classify(&mut last, Some(2), false, plain), Change::None);
    }

    #[test]
    fn pause_skips_without_reading_types() {
        let mut last = Some(1);
        let r = classify(&mut last, Some(2), true, || panic!("types lus pendant la pause"));
        assert_eq!(r, Change::Skip("pause"));
    }

    #[test]
    fn without_counter_always_reads() {
        let mut last = None;
        assert_eq!(classify(&mut last, None, false, plain), Change::Read);
    }
}
