//! Afficher / masquer la fenêtre.
//!
//! Sur macOS on passe par `NSApplication hide:/unhide:` : c'est l'équivalent de
//! Cmd+H, le focus revient automatiquement à l'application précédente (pratique
//! pour coller juste après avoir choisi un élément), et cela fonctionne même
//! quand egui ne redessine plus la fenêtre.

use eframe::egui;

#[cfg(target_os = "macos")]
mod imp {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};

    fn ns_app() -> *mut AnyObject {
        unsafe { msg_send![class!(NSApplication), sharedApplication] }
    }

    pub fn hide() {
        unsafe {
            let app = ns_app();
            let _: () = msg_send![app, hide: std::ptr::null_mut::<AnyObject>()];
        }
    }

    pub fn show() {
        unsafe {
            let app = ns_app();
            let _: () = msg_send![app, unhide: std::ptr::null_mut::<AnyObject>()];
            let _: () = msg_send![app, activateIgnoringOtherApps: true];
        }
    }

    /// Réaffiche l'app depuis n'importe quel thread (AppKit exige le thread principal).
    pub fn unhide_from_any_thread() {
        unsafe {
            let app = ns_app();
            let _: () = msg_send![
                app,
                performSelectorOnMainThread: objc2::sel!(unhide:),
                withObject: std::ptr::null_mut::<AnyObject>(),
                waitUntilDone: false
            ];
        }
    }

    /// Vrai si l'application est au premier plan et non masquée.
    pub fn is_frontmost() -> bool {
        unsafe {
            let app = ns_app();
            let active: bool = msg_send![app, isActive];
            let hidden: bool = msg_send![app, isHidden];
            active && !hidden
        }
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
