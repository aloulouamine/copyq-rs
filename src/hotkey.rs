//! Raccourci clavier global (⌘⇧Z par défaut) pour afficher / masquer l'historique.

use crate::{keyboard, platform};
use eframe::egui;
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::error::Error;

/// Enregistre le raccourci. Doit être appelée sur le thread principal.
/// Le `GlobalHotKeyManager` renvoyé doit rester vivant, sinon le raccourci est retiré.
pub fn register(ctx: &egui::Context) -> Result<GlobalHotKeyManager, Box<dyn Error>> {
    let manager = GlobalHotKeyManager::new()?;
    // On cherche la touche qui produit « z » avec la disposition active
    // (sur AZERTY, ce n'est pas la même position qu'en QWERTY).
    let key = keyboard::code_for_char('z').unwrap_or(Code::KeyZ);
    log::info!("Raccourci global : ⌘⇧Z (touche physique {key:?})");
    // META = touche ⌘ sur macOS.
    let hotkey = HotKey::new(Some(Modifiers::META | Modifiers::SHIFT), key);
    manager.register(hotkey)?;

    let id = hotkey.id();
    let ctx = ctx.clone();
    GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
        if event.id == id && event.state == HotKeyState::Pressed {
            platform::toggle_window(&ctx);
        }
    }));
    Ok(manager)
}
