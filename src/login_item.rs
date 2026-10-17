//! Lancement à l'ouverture de session via `SMAppService.mainAppService`
//! (macOS 13+). Ne fonctionne que pour une application empaquetée (`.app`) :
//! lancé comme simple binaire (`cargo run`), le service est « introuvable ».

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Status {
    Enabled,
    Disabled,
    /// L'utilisateur doit valider dans Réglages → Général → Ouverture.
    RequiresApproval,
    /// Binaire non empaqueté, ou macOS trop ancien.
    Unavailable,
}

#[cfg(target_os = "macos")]
mod imp {
    use super::Status;
    use objc2::msg_send;
    use objc2::rc::autoreleasepool;
    use objc2::runtime::{AnyClass, AnyObject};
    use std::ffi::{c_char, CStr};

    #[link(name = "ServiceManagement", kind = "framework")]
    extern "C" {}

    fn service() -> Option<*mut AnyObject> {
        let class = AnyClass::get("SMAppService")?;
        let svc: *mut AnyObject = unsafe { msg_send![class, mainAppService] };
        (!svc.is_null()).then_some(svc)
    }

    /// Vrai si le processus tourne depuis un paquet `.app` doté d'un identifiant.
    pub fn is_bundled() -> bool {
        autoreleasepool(|_| unsafe {
            let Some(class) = AnyClass::get("NSBundle") else { return false };
            let bundle: *mut AnyObject = msg_send![class, mainBundle];
            if bundle.is_null() {
                return false;
            }
            let id: *mut AnyObject = msg_send![bundle, bundleIdentifier];
            let path: *mut AnyObject = msg_send![bundle, bundlePath];
            !id.is_null() && nsstring(path).is_some_and(|p| p.ends_with(".app"))
        })
    }

    pub fn raw_status() -> Option<isize> {
        autoreleasepool(|_| {
            let svc = service()?;
            Some(unsafe { msg_send![svc, status] })
        })
    }

    pub fn status() -> Status {
        if !is_bundled() {
            return Status::Unavailable;
        }
        match raw_status() {
            Some(1) => Status::Enabled,
            Some(2) => Status::RequiresApproval,
            // 0 = non inscrite ; 3 = « introuvable », renvoyé tant que l'app
            // principale n'a jamais été inscrite : l'inscription reste possible.
            Some(0) | Some(3) => Status::Disabled,
            _ => Status::Unavailable,
        }
    }

    unsafe fn nsstring(s: *mut AnyObject) -> Option<String> {
        if s.is_null() {
            return None;
        }
        let c: *const c_char = msg_send![s, UTF8String];
        (!c.is_null()).then(|| CStr::from_ptr(c).to_string_lossy().into_owned())
    }

    pub fn set_enabled(enabled: bool) -> Result<(), String> {
        autoreleasepool(|_| {
            let svc = service().ok_or("lancement au démarrage indisponible sur ce macOS")?;
            let mut error: *mut AnyObject = std::ptr::null_mut();
            let ok: bool = unsafe {
                if enabled {
                    msg_send![svc, registerAndReturnError: &mut error as *mut *mut AnyObject]
                } else {
                    msg_send![svc, unregisterAndReturnError: &mut error as *mut *mut AnyObject]
                }
            };
            if ok {
                Ok(())
            } else {
                Err(describe(error))
            }
        })
    }

    fn describe(error: *mut AnyObject) -> String {
        if error.is_null() {
            return "erreur inconnue".into();
        }
        unsafe {
            let desc: *mut AnyObject = msg_send![error, localizedDescription];
            nsstring(desc).unwrap_or_else(|| "erreur inconnue".into())
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::Status;
    pub fn status() -> Status {
        Status::Unavailable
    }
    pub fn set_enabled(_: bool) -> Result<(), String> {
        Err("non pris en charge sur ce système".into())
    }
    pub fn is_bundled() -> bool {
        false
    }
    pub fn raw_status() -> Option<isize> {
        None
    }
}

pub use imp::{set_enabled, status};
use imp::{is_bundled, raw_status};

/// Auto-test lancé par la CI dans une app empaquetée : `copyq-rs --login-item-selftest`.
pub fn selftest() -> i32 {
    let before = status();
    println!(
        "app empaquetée : {} ; statut brut : {:?} ; statut : {before:?}",
        is_bundled(),
        raw_status()
    );
    if before == Status::Unavailable {
        println!("ÉCHEC : service indisponible (l'app est-elle bien un .app ?)");
        return 1;
    }
    let reg = set_enabled(true);
    let after_on = status();
    println!("statut brut après activation : {:?}", raw_status());
    println!("activation : {reg:?} → {after_on:?}");
    let unreg = set_enabled(false);
    let after_off = status();
    println!("désactivation : {unreg:?} → {after_off:?}");
    let ok = reg.is_ok()
        && matches!(after_on, Status::Enabled | Status::RequiresApproval)
        && unreg.is_ok()
        && after_off == Status::Disabled;
    println!("{}", if ok { "SUCCÈS" } else { "ÉCHEC" });
    if ok {
        0
    } else {
        1
    }
}
