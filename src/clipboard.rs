//! Surveillance du presse-papiers système dans un thread dédié.
//!
//! macOS ne fournit pas de notification de changement : on interroge le
//! presse-papiers à intervalle régulier, comme le fait CopyQ.

use crate::history::History;
use eframe::egui;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const POLL_INTERVAL: Duration = Duration::from_millis(400);

pub fn spawn_watcher(history: Arc<Mutex<History>>, paused: Arc<AtomicBool>, ctx: egui::Context) {
    thread::Builder::new()
        .name("clipboard-watcher".into())
        .spawn(move || {
            let mut clipboard = match arboard::Clipboard::new() {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Impossible d'accéder au presse-papiers : {e}");
                    return;
                }
            };
            let mut last: Option<String> = None;
            loop {
                // Erreur = presse-papiers vide ou contenu non textuel (image, fichier…) : ignoré.
                if let Ok(text) = clipboard.get_text() {
                    if last.as_deref() != Some(text.as_str()) {
                        last = Some(text.clone());
                        if !paused.load(Ordering::Relaxed) {
                            let changed = {
                                let mut h = history.lock().unwrap();
                                let changed = h.push(text);
                                if changed {
                                    if let Err(e) = h.save() {
                                        eprintln!("Échec de la sauvegarde : {e}");
                                    }
                                }
                                changed
                            };
                            if changed {
                                ctx.request_repaint();
                            }
                        }
                    }
                }
                thread::sleep(POLL_INTERVAL);
            }
        })
        .expect("impossible de lancer le thread de surveillance");
}
