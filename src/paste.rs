//! Collage automatique : après le choix d'un élément, on rend la main à
//! l'application où l'on travaillait et on y simule ⌘V.
//!
//! Simuler une frappe dans une autre application exige l'autorisation
//! **Accessibilité** (Réglages Système → Confidentialité et sécurité →
//! Accessibilité). Sans elle, l'élément est seulement copié.

use std::sync::atomic::{AtomicI32, AtomicU16, Ordering};
use std::thread;
use std::time::{Duration, Instant};

/// Application au premier plan au moment où l'historique a été ouvert (pid).
static PREVIOUS_APP: AtomicI32 = AtomicI32::new(0);
/// Code de la touche « V » pour la disposition du clavier (calculé au démarrage :
/// la lecture de la disposition doit se faire sur le thread principal).
static V_KEY: AtomicU16 = AtomicU16::new(9);

/// À appeler au démarrage, sur le thread principal.
pub fn init() {
    let vk = crate::keyboard::vk_for_char('v')
        .or_else(|| crate::keyboard::ansi_vk('v'))
        .unwrap_or(9);
    V_KEY.store(vk, Ordering::Relaxed);
}

/// À appeler juste *avant* d'afficher la fenêtre.
pub fn remember_frontmost() {
    if let Some(pid) = imp::frontmost_pid() {
        if pid != std::process::id() as i32 {
            PREVIOUS_APP.store(pid, Ordering::Relaxed);
        }
    }
}

pub fn previous_app() -> Option<i32> {
    let pid = PREVIOUS_APP.load(Ordering::Relaxed);
    (pid > 0).then_some(pid)
}

pub fn is_trusted() -> bool {
    imp::is_trusted()
}

/// Affiche la demande d'autorisation Accessibilité de macOS (une seule fois par session).
pub fn request_trust() {
    static ASKED: std::sync::Once = std::sync::Once::new();
    ASKED.call_once(imp::prompt_trust);
}

pub fn open_accessibility_settings() {
    if let Err(e) = std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        .spawn()
    {
        log::warn!("ouverture des réglages d'accessibilité impossible : {e}");
    }
}

#[derive(Debug, PartialEq)]
pub enum Outcome {
    /// ⌘V envoyé à l'application d'origine.
    Pasted,
    /// Pas d'autorisation Accessibilité : élément seulement copié.
    NotTrusted,
    /// Application d'origine inconnue ou fermée.
    NoTarget,
}

/// Réactive l'application d'origine puis y colle (dans un thread : ne bloque pas l'UI).
/// L'appelant doit avoir écrit le presse-papiers et masqué la fenêtre.
pub fn paste_into_previous() -> Outcome {
    if !imp::is_trusted() {
        request_trust();
        return Outcome::NotTrusted;
    }
    let Some(pid) = previous_app() else {
        return Outcome::NoTarget;
    };
    if !imp::activate(pid) {
        return Outcome::NoTarget;
    }
    thread::spawn(move || {
        // Attendre que l'application d'origine soit vraiment au premier plan :
        // coller ailleurs serait pire que ne rien faire.
        let deadline = Instant::now() + Duration::from_millis(800);
        while imp::frontmost_pid() != Some(pid) {
            if Instant::now() > deadline {
                log::warn!(
                    "collage annulé : l'application d'origine n'est pas revenue au premier plan"
                );
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
        // Laisser le champ de texte reprendre le focus.
        thread::sleep(Duration::from_millis(60));
        imp::send_cmd_v();
        log::info!("collé dans l'application d'origine (pid {pid})");
    });
    Outcome::Pasted
}

#[cfg(target_os = "macos")]
mod imp {
    use objc2::rc::autoreleasepool;
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    use std::ffi::c_void;

    type CFTypeRef = *const c_void;
    const CMD_FLAG: u64 = 0x0010_0000; // kCGEventFlagMaskCommand
    const HID_EVENT_TAP: u32 = 0; // kCGHIDEventTap
    const HID_SYSTEM_STATE: i32 = 1; // kCGEventSourceStateHIDSystemState

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
        fn AXIsProcessTrustedWithOptions(options: CFTypeRef) -> bool;
        static kAXTrustedCheckOptionPrompt: CFTypeRef;
        fn CGEventSourceCreate(state: i32) -> *mut c_void;
        fn CGEventCreateKeyboardEvent(source: *mut c_void, key: u16, down: bool) -> *mut c_void;
        fn CGEventSetFlags(event: *mut c_void, flags: u64);
        fn CGEventPost(tap: u32, event: *mut c_void);
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        static kCFBooleanTrue: CFTypeRef;
        static kCFTypeDictionaryKeyCallBacks: c_void;
        static kCFTypeDictionaryValueCallBacks: c_void;
        fn CFDictionaryCreate(
            allocator: CFTypeRef,
            keys: *const CFTypeRef,
            values: *const CFTypeRef,
            count: isize,
            key_callbacks: *const c_void,
            value_callbacks: *const c_void,
        ) -> CFTypeRef;
        fn CFRelease(cf: CFTypeRef);
    }

    pub fn is_trusted() -> bool {
        unsafe { AXIsProcessTrusted() }
    }

    pub fn prompt_trust() {
        unsafe {
            let keys = [kAXTrustedCheckOptionPrompt];
            let values = [kCFBooleanTrue];
            let dict = CFDictionaryCreate(
                std::ptr::null(),
                keys.as_ptr(),
                values.as_ptr(),
                1,
                &kCFTypeDictionaryKeyCallBacks,
                &kCFTypeDictionaryValueCallBacks,
            );
            let trusted = AXIsProcessTrustedWithOptions(dict);
            if !dict.is_null() {
                CFRelease(dict);
            }
            log::info!("autorisation Accessibilité demandée (déjà accordée : {trusted})");
        }
    }

    pub fn frontmost_pid() -> Option<i32> {
        autoreleasepool(|_| unsafe {
            let ws: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
            let app: *mut AnyObject = msg_send![ws, frontmostApplication];
            if app.is_null() {
                return None;
            }
            let pid: i32 = msg_send![app, processIdentifier];
            Some(pid)
        })
    }

    pub fn activate(pid: i32) -> bool {
        autoreleasepool(|_| unsafe {
            let app: *mut AnyObject = msg_send![
                class!(NSRunningApplication),
                runningApplicationWithProcessIdentifier: pid
            ];
            if app.is_null() {
                return false;
            }
            // NSApplicationActivateIgnoringOtherApps (2) : ignoré depuis macOS 14,
            // où l'activation suit simplement le masquage de notre fenêtre.
            msg_send![app, activateWithOptions: 2usize]
        })
    }

    pub fn send_cmd_v() {
        // Touche qui produit « v » avec la disposition active (AZERTY, Dvorak…).
        let vk = super::V_KEY.load(std::sync::atomic::Ordering::Relaxed);
        unsafe {
            let source = CGEventSourceCreate(HID_SYSTEM_STATE);
            for down in [true, false] {
                let ev = CGEventCreateKeyboardEvent(source, vk, down);
                if ev.is_null() {
                    continue;
                }
                CGEventSetFlags(ev, CMD_FLAG);
                CGEventPost(HID_EVENT_TAP, ev);
                CFRelease(ev as CFTypeRef);
            }
            if !source.is_null() {
                CFRelease(source as CFTypeRef);
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    pub fn is_trusted() -> bool {
        false
    }
    pub fn prompt_trust() {}
    pub fn frontmost_pid() -> Option<i32> {
        None
    }
    pub fn activate(_: i32) -> bool {
        false
    }
    pub fn send_cmd_v() {}
}

/// Auto-test CI : `copyq-rs --paste-selftest <texte>` — écrit le texte dans le
/// presse-papiers et le colle dans l'application au premier plan.
pub fn selftest(text: &str) -> i32 {
    println!("autorisation Accessibilité : {}", is_trusted());
    if !is_trusted() {
        println!("IGNORÉ : pas d'autorisation Accessibilité dans cet environnement");
        return 2;
    }
    init();
    remember_frontmost();
    let Some(pid) = previous_app() else {
        println!("ÉCHEC : aucune application au premier plan");
        return 1;
    };
    if let Err(e) = arboard::Clipboard::new().and_then(|mut c| c.set_text(text.to_string())) {
        println!("ÉCHEC presse-papiers : {e}");
        return 1;
    }
    let outcome = paste_into_previous();
    thread::sleep(Duration::from_millis(1200));
    println!("cible pid {pid} → {outcome:?}");
    i32::from(outcome != Outcome::Pasted)
}
