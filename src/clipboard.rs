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

pub fn spawn_watcher(
    history: Arc<Mutex<History>>,
    paused: Arc<AtomicBool>,
    save_passwords: Arc<AtomicBool>,
    ctx: egui::Context,
) {
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
                let change = classify(
                    &mut last_count,
                    count,
                    paused.load(Ordering::Relaxed),
                    save_passwords.load(Ordering::Relaxed),
                    pasteboard::types,
                );
                let sensitive = match change {
                    Change::None => continue,
                    Change::Skip(reason) => {
                        log::info!("Copie ignorée ({reason})");
                        continue;
                    }
                    Change::Read { sensitive } => sensitive,
                };
                // Erreur = contenu non textuel (image, fichier…) : ignoré.
                let Ok(text) = clipboard.get_text() else { continue };
                if count.is_none() {
                    if last_text.as_deref() == Some(text.as_str()) {
                        continue;
                    }
                    last_text = Some(text.clone());
                }
                if sensitive {
                    log::info!("Mot de passe enregistré (réglage activé), affiché masqué");
                }
                let changed = history.lock_safe().push_with(text, sensitive);
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
        assert_eq!(classify(&mut last, Some(5), false, false, plain), Change::None);
    }

    #[test]
    fn new_counter_reads() {
        let mut last = Some(5);
        assert_eq!(classify(&mut last, Some(6), false, false, plain), Change::Read { sensitive: false });
        assert_eq!(last, Some(6));
        assert_eq!(classify(&mut last, Some(6), false, false, plain), Change::None);
    }

    #[test]
    fn concealed_is_skipped_and_not_reread() {
        let mut last = Some(1);
        let secret = || vec!["org.nspasteboard.ConcealedType".to_string()];
        assert!(matches!(classify(&mut last, Some(2), false, false, secret), Change::Skip(_)));
        // Le compteur a été mémorisé : on ne retente pas sur le même contenu.
        assert_eq!(classify(&mut last, Some(2), false, false, plain), Change::None);
    }

    #[test]
    fn pause_skips_without_reading_types() {
        let mut last = Some(1);
        let r = classify(&mut last, Some(2), true, false, || panic!("types lus pendant la pause"));
        assert_eq!(r, Change::Skip("pause"));
    }

    #[test]
    fn cleared_but_not_yet_written_is_retried() {
        let mut last = Some(1);
        // L'app source vient de vider le presse-papiers : rien à lire encore.
        assert_eq!(classify(&mut last, Some(2), false, false, Vec::new), Change::None);
        assert_eq!(last, Some(1), "le compteur ne doit pas être mémorisé");
        // Le contenu est écrit (même compteur) : on le lit bien.
        assert_eq!(classify(&mut last, Some(2), false, false, plain), Change::Read { sensitive: false });
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
        assert_eq!(classify(&mut last, None, false, false, plain), Change::Read { sensitive: false });
    }
}
