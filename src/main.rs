//! CopyQ-rs : gestionnaire d'historique du presse-papiers inspiré de CopyQ.

mod app;
mod clipboard;
mod history;
mod hotkey;
mod icons;
mod keyboard;
mod logging;
mod pasteboard;
mod platform;
mod storage;
mod theme;
mod tray;

use eframe::egui;
use std::sync::{Arc, Mutex};

fn main() -> eframe::Result<()> {
    logging::init();
    let history = match storage::Store::open(
        storage::default_db_path(),
        Some(history::legacy_json_path()),
    ) {
        Ok((store, items)) => history::History::with_store(store, items),
        Err(e) => {
            // L'app reste utilisable (historique en mémoire seulement) plutôt que de planter.
            log::error!("base indisponible, historique non sauvegardé : {e}");
            history::History::default()
        }
    };
    let history = Arc::new(Mutex::new(history));

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("CopyQ-rs")
            .with_inner_size([540.0, 640.0])
            .with_min_inner_size([380.0, 360.0])
            // Barre de titre transparente : le contenu va jusqu'en haut,
            // seuls les trois boutons de fenêtre macOS restent visibles.
            .with_fullsize_content_view(true)
            .with_titlebar_shown(false)
            .with_title_shown(false),
        ..Default::default()
    };

    eframe::run_native(
        "CopyQ-rs",
        options,
        Box::new(move |cc| Ok(Box::new(app::CopyApp::new(cc, history)))),
    )
}
