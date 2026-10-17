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

    pub fn status() -> Status {
        autoreleasepool(|_| {
            let Some(svc) = service() else {
                return Status::Unavailable;
            };
            let raw: isize = unsafe { msg_send![svc, status] };
            match raw {
                1 => Status::Enabled,
                2 => Status::RequiresApproval,
                0 => Status::Disabled,
                _ => Status::Unavailable,
            }
        })
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
            if desc.is_null() {
                return "erreur inconnue".into();
            }
            let c: *const c_char = msg_send![desc, UTF8String];
            if c.is_null() {
                "erreur inconnue".into()
            } else {
                CStr::from_ptr(c).to_string_lossy().into_owned()
            }
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
}

pub use imp::{set_enabled, status};

/// Auto-test lancé par la CI dans une app empaquetée : `copyq-rs --login-item-selftest`.
pub fn selftest() -> i32 {
    let before = status();
    println!("statut initial : {before:?}");
    if before == Status::Unavailable {
        println!("ÉCHEC : service indisponible (l'app est-elle bien un .app ?)");
        return 1;
    }
    let reg = set_enabled(true);
    let after_on = status();
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
