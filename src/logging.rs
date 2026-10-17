//! Journal de l'application et filet de sécurité contre les plantages.
//!
//! - Les messages vont dans `~/Library/Logs/copyq-rs/copyq-rs.log` (et dans le
//!   terminal), le fichier est remis à zéro s'il dépasse 1 Mo.
//! - Un panic dans n'importe quel thread est consigné avec sa position.
//! - `LockExt::lock_safe` récupère un verrou « empoisonné » au lieu de planter
//!   en cascade : un thread qui a paniqué ne fait pas tomber toute l'app.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

const MAX_LOG_BYTES: u64 = 1_000_000;

pub trait LockExt<T> {
    fn lock_safe(&self) -> MutexGuard<'_, T>;
}

impl<T> LockExt<T> for Mutex<T> {
    fn lock_safe(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|poisoned| {
            log::error!("verrou récupéré après un panic dans un autre thread");
            poisoned.into_inner()
        })
    }
}

struct Logger {
    file: Option<Mutex<File>>,
}

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!(
            "{} [{}] {}\n",
            crate::history::now_secs(),
            record.level(),
            record.args()
        );
        eprint!("{line}");
        if let Some(file) = &self.file {
            if let Ok(mut f) = file.lock() {
                let _ = f.write_all(line.as_bytes());
            }
        }
    }

    fn flush(&self) {}
}

pub fn log_path() -> PathBuf {
    directories::BaseDirs::new()
        .map(|d| d.home_dir().join("Library/Logs/copyq-rs/copyq-rs.log"))
        .unwrap_or_else(|| PathBuf::from("copyq-rs.log"))
}

/// Installe le journal et le gestionnaire de panic. À appeler en tout premier.
pub fn init() {
    let path = log_path();
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if fs::metadata(&path)
        .map(|m| m.len() > MAX_LOG_BYTES)
        .unwrap_or(false)
    {
        let _ = fs::rename(&path, path.with_extension("log.old"));
    }
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok()
        .map(Mutex::new);

    let logger = Box::leak(Box::new(Logger { file }));
    if log::set_logger(logger).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }

    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        log::error!(
            "PANIC dans le thread « {} » : {info}",
            thread.name().unwrap_or("?")
        );
        default_hook(info);
    }));

    log::info!("CopyQ-rs {} démarré", env!("CARGO_PKG_VERSION"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn poisoned_lock_is_recovered() {
        let m = Arc::new(Mutex::new(41));
        let m2 = m.clone();
        let _ = std::thread::spawn(move || {
            let _g = m2.lock().unwrap();
            panic!("panic volontaire pendant que le verrou est tenu");
        })
        .join();
        assert!(m.lock().is_err(), "le verrou devrait être empoisonné");
        *m.lock_safe() += 1;
        assert_eq!(*m.lock_safe(), 42);
    }
}
