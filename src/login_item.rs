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
    use objc2::rc::{autoreleasepool, Retained};
    use objc2::runtime::AnyClass;
    use objc2_foundation::NSBundle;
    use objc2_service_management::SMAppService;

    /// Service de l'app principale ; `None` avant macOS 13 (classe absente).
    fn service() -> Option<Retained<SMAppService>> {
        AnyClass::get(c"SMAppService")?;
        // SAFETY : la classe existe (vérifié ci-dessus) ; méthode de classe sans argument.
        Some(unsafe { SMAppService::mainAppService() })
    }

    /// Vrai si le processus tourne depuis un paquet `.app` doté d'un identifiant.
    pub fn is_bundled() -> bool {
        autoreleasepool(|_| {
            let bundle = NSBundle::mainBundle();
            bundle.bundleIdentifier().is_some() && bundle.bundlePath().to_string().ends_with(".app")
        })
    }

    pub fn raw_status() -> Option<isize> {
        autoreleasepool(|_| {
            let svc = service()?;
            // SAFETY : simple lecture de l'état du service.
            Some(unsafe { svc.status() }.0)
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
            Some(0 | 3) => Status::Disabled,
            _ => Status::Unavailable,
        }
    }

    pub fn set_enabled(enabled: bool) -> Result<(), String> {
        autoreleasepool(|_| {
            let svc = service().ok_or("lancement au démarrage indisponible sur ce macOS")?;
            // SAFETY : (dés)inscription de l'app elle-même ; l'erreur éventuelle est
            // renvoyée sous forme de `NSError`.
            let result = unsafe {
                if enabled {
                    svc.registerAndReturnError()
                } else {
                    svc.unregisterAndReturnError()
                }
            };
            result.map_err(|e| e.localizedDescription().to_string())
        })
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

use imp::{is_bundled, raw_status};
pub use imp::{set_enabled, status};

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
