# CopyQ-rs

Gestionnaire d'historique du presse-papiers pour macOS, inspiré de [CopyQ](https://hluk.github.io/CopyQ/), écrit en Rust avec [egui](https://github.com/emilk/egui).

## Fonctionnalités (v0.1 — MVP)

- Enregistre automatiquement tout texte copié (500 derniers éléments, doublons remontés en tête)
- Historique persistant : `~/Library/Application Support/com.amine.copyq-rs/history.json`
- Recherche instantanée (tous les mots, insensible à la casse)
- Clic ou **Entrée** : remet l'élément dans le presse-papiers et masque la fenêtre → il suffit de faire ⌘V
- Navigation **↑ / ↓**, **⌘⌫** supprime l'élément sélectionné, **Échap** vide la recherche puis masque
- **⌘⇧Z** (raccourci global) affiche l'historique, ou le masque s'il est déjà au premier plan
- Icône dans la barre des menus (Afficher l'historique / Quitter) ; fermer la fenêtre la masque seulement
- Thème moderne clair / sombre qui suit le réglage de macOS
- Interrupteur **Pause** pour ne pas enregistrer ce que vous copiez (mots de passe…)

## Lancer

```bash
# Rust requis : https://rustup.rs
cargo run --release
```

Créer une vraie application `.app` :

```bash
cargo install cargo-bundle
cargo bundle --release
open target/release/bundle/osx/CopyQ-rs.app
```

Tests : `cargo test`

## Architecture

| Fichier | Rôle |
|---|---|
| `src/main.rs` | Démarrage, fenêtre eframe |
| `src/history.rs` | Modèle de l'historique, recherche, sauvegarde JSON (testé) |
| `src/clipboard.rs` | Thread qui interroge le presse-papiers toutes les 400 ms |
| `src/app.rs` | Interface egui et raccourcis clavier |
| `src/theme.rs` | Thème clair / sombre (suit macOS), police SF Pro |
| `src/hotkey.rs` | Raccourci global ⌘⇧Z |
| `src/keyboard.rs` | Trouve la touche « Z » selon la disposition (AZERTY, QWERTZ…) |
| `src/tray.rs` | Icône et menu de la barre des menus |
| `src/platform.rs` | Afficher/masquer via `NSApplication hide:/unhide:` sur macOS |

## Pistes pour la suite

- Raccourci global configurable
- Éléments épinglés, onglets
- Images (arboard sait lire les images)
- Collage automatique après sélection (nécessite l'autorisation Accessibilité)
- Masquer l'icône du Dock (`LSUIElement`)
