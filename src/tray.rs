//! Icône dans la barre des menus (macOS) / zone de notification.

use crate::platform;
use eframe::egui;
use std::error::Error;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

/// Crée l'icône. Doit être appelée sur le thread principal, une fois la boucle
/// d'événements démarrée (c'est le cas dans le « creator » d'eframe).
/// Le `TrayIcon` renvoyé doit rester vivant : le détruire retire l'icône.
pub fn create(ctx: &egui::Context) -> Result<TrayIcon, Box<dyn Error>> {
    let menu = Menu::new();
    let show = MenuItem::new("Afficher l'historique", true, None);
    let quit = MenuItem::new("Quitter", true, None);
    menu.append(&show)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&quit)?;

    let show_id = show.id().clone();
    let quit_id = quit.id().clone();
    let ctx = ctx.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if event.id == show_id {
            platform::show_window(&ctx);
        } else if event.id == quit_id {
            // Termine les écritures en attente avant de quitter.
            crate::storage::close_global();
            log::info!("fermeture demandée depuis la barre des menus");
            std::process::exit(0);
        }
    }));

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("CopyQ-rs")
        // Gabarit : macOS adapte la couleur au thème de la barre des menus.
        .with_icon_templated(icon()?)
        .build()?;
    Ok(tray)
}

/// Dessine une petite icône « presse-papiers » 32×32 (monochrome, adaptée au mode
/// template de macOS qui ne garde que la transparence).
fn icon() -> Result<Icon, Box<dyn Error>> {
    const S: u32 = 32;
    let mut rgba = vec![0u8; (S * S * 4) as usize];
    let mut set = |x: u32, y: u32| {
        let i = ((y * S + x) * 4) as usize;
        rgba[i..i + 4].copy_from_slice(&[0, 0, 0, 255]);
    };
    for y in 0..S {
        for x in 0..S {
            // Corps : contour épais de 2 px.
            let body = (6..=25).contains(&x) && (5..=29).contains(&y);
            let inner = (8..=23).contains(&x) && (7..=27).contains(&y);
            // Pince en haut.
            let clip = (11..=20).contains(&x) && (2..=7).contains(&y);
            // Lignes de texte.
            let line = (11..=20).contains(&x) && matches!(y, 13 | 14 | 18 | 19 | 23 | 24);
            if (body && !inner) || clip || line {
                set(x, y);
            }
        }
    }
    Ok(Icon::from_rgba(rgba, S, S)?)
}
