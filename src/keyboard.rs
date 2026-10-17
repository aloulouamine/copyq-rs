//! Correspondance caractère → touche physique selon la disposition du clavier.
//!
//! Les raccourcis globaux macOS sont définis par la *position* de la touche
//! (code virtuel QWERTY), pas par la lettre imprimée dessus. Sur un clavier
//! AZERTY, la position « Z » du QWERTY porte la lettre W. On interroge donc la
//! disposition active pour trouver la touche qui produit réellement le caractère.

use global_hotkey::hotkey::Code;

/// Codes virtuels macOS des touches lettres (positions ANSI).
const LETTER_KEYS: [(u16, Code); 26] = [
    (0x00, Code::KeyA),
    (0x0B, Code::KeyB),
    (0x08, Code::KeyC),
    (0x02, Code::KeyD),
    (0x0E, Code::KeyE),
    (0x03, Code::KeyF),
    (0x05, Code::KeyG),
    (0x04, Code::KeyH),
    (0x22, Code::KeyI),
    (0x26, Code::KeyJ),
    (0x28, Code::KeyK),
    (0x25, Code::KeyL),
    (0x2E, Code::KeyM),
    (0x2D, Code::KeyN),
    (0x1F, Code::KeyO),
    (0x23, Code::KeyP),
    (0x0C, Code::KeyQ),
    (0x0F, Code::KeyR),
    (0x01, Code::KeyS),
    (0x11, Code::KeyT),
    (0x20, Code::KeyU),
    (0x09, Code::KeyV),
    (0x0D, Code::KeyW),
    (0x07, Code::KeyX),
    (0x10, Code::KeyY),
    (0x06, Code::KeyZ),
];

/// Touche physique qui produit `ch` avec la disposition active, ou `None` si inconnue.
pub fn code_for_char(ch: char) -> Option<Code> {
    let target = ch.to_lowercase().next()?;
    LETTER_KEYS
        .iter()
        .find(|(vk, _)| imp::translate(*vk).and_then(|c| c.to_lowercase().next()) == Some(target))
        .map(|(_, code)| *code)
}

#[cfg(target_os = "macos")]
mod imp {
    use std::ffi::c_void;

    type CFTypeRef = *const c_void;

    #[link(name = "Carbon", kind = "framework")]
    extern "C" {
        fn TISCopyCurrentKeyboardLayoutInputSource() -> CFTypeRef;
        fn TISCopyCurrentASCIICapableKeyboardLayoutInputSource() -> CFTypeRef;
        fn TISGetInputSourceProperty(source: CFTypeRef, key: CFTypeRef) -> CFTypeRef;
        static kTISPropertyUnicodeKeyLayoutData: CFTypeRef;
        fn LMGetKbdType() -> u8;
        fn UCKeyTranslate(
            layout: *const c_void,
            virtual_key_code: u16,
            key_action: u16,
            modifier_key_state: u32,
            keyboard_type: u32,
            key_translate_options: u32,
            dead_key_state: *mut u32,
            max_string_length: usize,
            actual_string_length: *mut usize,
            unicode_string: *mut u16,
        ) -> i32;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFDataGetBytePtr(data: CFTypeRef) -> *const u8;
        fn CFRelease(cf: CFTypeRef);
    }

    const K_UC_KEY_ACTION_DISPLAY: u16 = 3;
    const K_UC_KEY_TRANSLATE_NO_DEAD_KEYS: u32 = 1;

    /// Caractère produit par la touche `vk` sans modificateur.
    /// Doit être appelée sur le thread principal (exigence de Text Input Sources).
    pub fn translate(vk: u16) -> Option<char> {
        unsafe {
            for copy in [
                TISCopyCurrentKeyboardLayoutInputSource as unsafe extern "C" fn() -> CFTypeRef,
                TISCopyCurrentASCIICapableKeyboardLayoutInputSource,
            ] {
                let source = copy();
                if source.is_null() {
                    continue;
                }
                let data = TISGetInputSourceProperty(source, kTISPropertyUnicodeKeyLayoutData);
                let result = if data.is_null() {
                    None
                } else {
                    let layout = CFDataGetBytePtr(data) as *const c_void;
                    let mut dead = 0u32;
                    let mut len = 0usize;
                    let mut buf = [0u16; 4];
                    let status = UCKeyTranslate(
                        layout,
                        vk,
                        K_UC_KEY_ACTION_DISPLAY,
                        0,
                        LMGetKbdType() as u32,
                        K_UC_KEY_TRANSLATE_NO_DEAD_KEYS,
                        &mut dead,
                        buf.len(),
                        &mut len,
                        buf.as_mut_ptr(),
                    );
                    if status == 0 && len > 0 {
                        char::decode_utf16(buf[..len].iter().copied())
                            .next()
                            .and_then(Result::ok)
                    } else {
                        None
                    }
                };
                CFRelease(source);
                if result.is_some() {
                    return result;
                }
            }
            None
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    pub fn translate(_vk: u16) -> Option<char> {
        None
    }
}
