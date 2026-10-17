//! Afficher / masquer la fenêtre.
//!
//! Sur macOS on passe par `NSApplication hide:/unhide:` : c'est l'équivalent de
//! Cmd+H, le focus revient automatiquement à l'application précédente (pratique
//! pour coller juste après avoir choisi un élément), et cela fonctionne même
//! quand egui ne redessine plus la fenêtre.

use eframe::egui;

#[cfg(target_os = "macos")]
mod imp {
    use objc2::{sel, MainThreadMarker};
    use objc2_app_kit::NSApplication;
    use objc2_foundation::NSObjectNSThreadPerformAdditions;

    /// L'application AppKit ; `None` hors du thread principal (AppKit l'interdit).
    fn app() -> Option<objc2::rc::Retained<NSApplication>> {
        let mtm = MainThreadMarker::new();
        if mtm.is_none() {
            log::warn!("fenêtre : appel hors du thread principal ignoré");
        }
        mtm.map(NSApplication::sharedApplication)
    }

    pub fn hide() {
        if let Some(app) = app() {
            app.hide(None);
        }
    }

    pub fn show() {
        if let Some(app) = app() {
            app.unhide(None);
            // Seule façon de passer devant l'app active avant macOS 14.
            #[allow(deprecated)]
            app.activateIgnoringOtherApps(true);
        }
    }

    /// Réaffiche l'app depuis n'importe quel thread (AppKit exige le thread principal).
    pub fn unhide_from_any_thread() {
        // `sharedApplication` sans marqueur de thread principal : l'objet est seulement
        // utilisé pour programmer `unhide:` sur le thread principal.
        let app: objc2::rc::Retained<NSApplication> =
            unsafe { objc2::msg_send![objc2::class!(NSApplication), sharedApplication] };
        // SAFETY : `unhide:` prend un objet facultatif (nil) et sera exécuté sur le
        // thread principal par AppKit.
        unsafe {
            app.performSelectorOnMainThread_withObject_waitUntilDone(sel!(unhide:), None, false);
        }
    }

    /// Vrai si l'application est au premier plan et non masquée.
    pub fn is_frontmost() -> bool {
        app().is_some_and(|app| app.isActive() && !app.isHidden())
    }
}

pub fn hide_window(ctx: &egui::Context) {
    #[cfg(target_os = "macos")]
    {
        let _ = ctx;
        imp::hide();
    }
    #[cfg(not(target_os = "macos"))]
    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
}

pub fn show_window(ctx: &egui::Context) {
    // Retenir où l'on travaillait, pour y coller l'élément choisi.
    crate::paste::remember_frontmost();
    #[cfg(target_os = "macos")]
    imp::show();
    #[cfg(not(target_os = "macos"))]
    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));

    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    ctx.request_repaint();
}

/// Affiche la fenêtre, ou la masque si elle est déjà au premier plan (comme CopyQ).
pub fn toggle_window(ctx: &egui::Context) {
    #[cfg(target_os = "macos")]
    if imp::is_frontmost() {
        hide_window(ctx);
        return;
    }
    show_window(ctx);
}

/// Comme `show_window`, mais utilisable hors du thread principal (ex. socket
/// d'instance unique). Le passage au premier plan est fait par egui (Focus).
pub fn show_window_from_any_thread(ctx: &egui::Context) {
    crate::paste::remember_frontmost();
    #[cfg(target_os = "macos")]
    imp::unhide_from_any_thread();
    #[cfg(not(target_os = "macos"))]
    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));

    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    ctx.request_repaint();
}
