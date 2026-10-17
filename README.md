# CopyQ-rs

Gestionnaire d'historique du presse-papiers pour macOS, inspiré de [CopyQ](https://hluk.github.io/CopyQ/), écrit en Rust avec [egui](https://github.com/emilk/egui).

## Fonctionnalités

- Enregistre automatiquement tout texte copié (500 derniers éléments, doublons remontés en tête)
- Historique persistant dans une base [Turso](https://github.com/tursodatabase/turso) (SQLite réécrit en Rust) : `~/Library/Application Support/com.amine.copyq-rs/history.db` — l'ancien `history.json` est importé automatiquement
- Recherche instantanée (tous les mots, insensible à la casse)
- Clic ou **Entrée** : remet l'élément dans le presse-papiers et masque la fenêtre → il suffit de faire ⌘V
- Navigation **↑ / ↓**, **⌘⌫** supprime l'élément sélectionné, **Échap** vide la recherche puis masque
- **⌘⇧Z** (raccourci global) affiche l'historique, ou le masque s'il est déjà au premier plan
- Icône dans la barre des menus (Afficher l'historique / Quitter) ; fermer la fenêtre la masque seulement
- Thème moderne clair / sombre qui suit le réglage de macOS
- Interrupteur **Pause** pour ne pas enregistrer ce que vous copiez
- Panneau **Réglages** : masquer après copie, ouvrir au démarrage (nécessite l'app empaquetée `.app`), enregistrer les mots de passe — conservés dans `settings.json`

## Robustesse

- **Économe** : seul le compteur de changements du presse-papiers est consulté (toutes les 300 ms) ; le contenu n'est lu que lorsqu'il change
- **Mots de passe ignorés par défaut** : les copies marquées confidentielles ou temporaires (1Password, Bitwarden, trousseau… — convention nspasteboard.org) ne sont ni lues ni enregistrées. Réglable dans **Réglages → Enregistrer les mots de passe** : ils sont alors gardés, affichés masqués (••••) et exclus de la recherche ; un bouton permet de tous les effacer
- **Mots de passe chiffrés** : AES-256-GCM (nonce aléatoire par élément), clé de 256 bits générée au premier besoin et conservée dans le **trousseau macOS** (service `com.amine.copyq-rs`, compte `history-encryption-key`) ; doublons et suppressions via une empreinte HMAC-SHA256. Si le trousseau est inaccessible, un mot de passe n'est **jamais** écrit en clair (il reste en mémoire). Les mots de passe stockés en clair par la v0.2 sont chiffrés au lancement.
- **Pas de plantage en cascade** : verrous récupérables, panics consignés
- **Journal** : `~/Library/Logs/copyq-rs/copyq-rs.log`
- **Base dans un thread dédié** : l'interface n'attend jamais le disque ; tout est écrit avant de quitter
- **Instance unique** : relancer l'app affiche la fenêtre déjà ouverte

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

Chaque `git push` lance sur un Mac GitHub (macOS 14) : tests unitaires, tests de bout en bout avec le vrai presse-papiers (`ci/e2e.py`), auto-test de l'app empaquetée (`ci/bundle_selftest.sh`) et captures d'écran clair/sombre. Résultats sur la branche `ci-results`.

## Architecture

| Fichier | Rôle |
|---|---|
| `src/main.rs` | Démarrage, fenêtre eframe |
| `src/history.rs` | Historique en mémoire, recherche, import de l'ancien JSON |
| `src/storage.rs` | Base Turso dans un thread dédié (seul module à faire du SQL) |
| `src/clipboard.rs` | Surveillance du presse-papiers (compteur de changements) |
| `src/pasteboard.rs` | Accès NSPasteboard : compteur, types, filtre des mots de passe |
| `src/logging.rs` | Journal fichier, gestion des panics, verrous récupérables |
| `src/single_instance.rs` | Instance unique (verrou + socket Unix) |
| `src/login_item.rs` | Ouverture au démarrage (`SMAppService`) |
| `src/settings.rs` | Réglages persistants (`settings.json`) |
| `src/crypto.rs` | Chiffrement AES-256-GCM et empreintes HMAC |
| `src/keychain.rs` | Clé de chiffrement dans le trousseau macOS |
| `src/app.rs` | Interface egui et raccourcis clavier |
| `src/icons.rs` | Icônes vectorielles |
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
