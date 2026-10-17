//! Une seule instance à la fois.
//!
//! La première instance prend un verrou exclusif sur `instance.lock` et écoute sur
//! une socket Unix. Une instance lancée ensuite ne peut pas prendre le verrou :
//! elle demande à la première d'afficher sa fenêtre, puis se termine.

use crate::platform;
use eframe::egui;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::thread;

const SHOW: &str = "show";

pub enum Instance {
    /// Nous sommes la première instance.
    Primary(Primary),
    /// Une autre instance tourne déjà (elle a été prévenue).
    Secondary,
}

pub struct Primary {
    /// Garder le fichier ouvert conserve le verrou jusqu'à la fin du processus.
    lock: Option<File>,
    listener: Option<UnixListener>,
}

pub fn acquire(dir: &Path) -> Instance {
    if let Err(e) = fs::create_dir_all(dir) {
        log::warn!("dossier de données inaccessible ({e}) : instance unique désactivée");
    }
    let lock_path = dir.join("instance.lock");
    let socket_path = dir.join("instance.sock");

    let lock = match OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&lock_path)
    {
        Ok(f) => f,
        Err(e) => {
            log::warn!("verrou d'instance impossible ({e}) : on continue sans");
            return Instance::Primary(Primary {
                lock: None,
                listener: None,
            });
        }
    };

    match lock.try_lock() {
        Ok(()) => {
            // Une socket restée d'un arrêt brutal est obsolète : nous avons le verrou.
            let _ = fs::remove_file(&socket_path);
            let listener = UnixListener::bind(&socket_path)
                .map_err(|e| log::warn!("socket d'instance indisponible : {e}"))
                .ok();
            Instance::Primary(Primary {
                lock: Some(lock),
                listener,
            })
        }
        Err(_) => {
            notify_running(&socket_path);
            Instance::Secondary
        }
    }
}

fn notify_running(socket_path: &Path) {
    match UnixStream::connect(socket_path) {
        Ok(mut stream) => match writeln!(stream, "{SHOW}") {
            Ok(()) => log::info!("CopyQ-rs est déjà lancé : fenêtre existante affichée"),
            Err(e) => log::warn!("CopyQ-rs est déjà lancé mais n'a pas reçu la demande : {e}"),
        },
        Err(e) => log::warn!("CopyQ-rs est déjà lancé mais injoignable : {e}"),
    }
}

impl Primary {
    /// Écoute les demandes des instances suivantes (dans un thread dédié).
    pub fn serve(self, ctx: egui::Context) {
        let Primary { lock, listener } = self;
        let Some(listener) = listener else {
            std::mem::forget(lock);
            return;
        };
        let spawned = thread::Builder::new()
            .name("single-instance".into())
            .spawn(move || {
                let _lock = lock;
                for stream in listener.incoming().flatten() {
                    let mut line = String::new();
                    if BufReader::new(stream).read_line(&mut line).is_ok() && line.trim() == SHOW {
                        log::info!("nouveau lancement détecté : affichage de la fenêtre");
                        platform::show_window_from_any_thread(&ctx);
                    }
                }
            });
        if let Err(e) = spawned {
            log::warn!("écoute d'instance impossible : {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_acquire_is_secondary() {
        let dir = std::env::temp_dir().join(format!("copyq-rs-instance-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let first = acquire(&dir);
        assert!(matches!(first, Instance::Primary(_)));
        // Même processus, autre descripteur : le verrou flock est déjà pris.
        assert!(matches!(acquire(&dir), Instance::Secondary));
        drop(first);
        assert!(
            matches!(acquire(&dir), Instance::Primary(_)),
            "verrou libéré à la fermeture"
        );
        let _ = fs::remove_dir_all(dir);
    }
}
