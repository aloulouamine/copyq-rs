//! CopyQ-rs : gestionnaire d'historique du presse-papiers inspiré de CopyQ.

mod app;
mod clipboard;
mod crypto;
mod history;
mod hotkey;
mod icons;
mod keyboard;
mod keychain;
mod logging;
mod media;
mod login_item;
mod pasteboard;
mod platform;
mod settings;
mod single_instance;
mod storage;
mod theme;
mod tray;

use eframe::egui;
use std::sync::{Arc, Mutex};

fn main() -> eframe::Result<()> {
    if std::env::args().any(|a| a == "--login-item-selftest") {
        std::process::exit(login_item::selftest());
    }
    if std::env::args().any(|a| a == "--keychain-selftest") {
        std::process::exit(keychain::selftest());
    }
    if std::env::args().any(|a| a == "--media-selftest") {
        std::process::exit(clipboard::media_selftest());
    }
    logging::init();
    log::info!("lancement au démarrage : {:?}", login_item::status());

    let instance = match single_instance::acquire(&history::data_dir()) {
        single_instance::Instance::Primary(p) => p,
        // Une autre instance tourne : elle vient d'afficher sa fenêtre.
        single_instance::Instance::Secondary => return Ok(()),
    };
    let history = match storage::Store::open(
        storage::default_db_path(),
        Some(history::legacy_json_path()),
        Some(media::images_dir()),
        // La clé n'est demandée au trousseau qu'au premier mot de passe à (dé)chiffrer.
        Box::new(keychain::load_or_create_key),
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
        Box::new(move |cc| {
            instance.serve(cc.egui_ctx.clone());
            Ok(Box::new(app::CopyApp::new(cc, history)))
        }),
    )
}
