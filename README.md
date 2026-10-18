# CopyQ-rs

Gestionnaire d'historique du presse-papiers pour macOS, inspiré de [CopyQ](https://hluk.github.io/CopyQ/), écrit en Rust avec [egui](https://github.com/emilk/egui).

## Fonctionnalités

- Enregistre automatiquement tout texte copié (500 derniers éléments, doublons remontés en tête)
- Historique persistant dans une base [Turso](https://github.com/tursodatabase/turso) (SQLite réécrit en Rust) : `~/Library/Application Support/com.amine.copyq-rs/history.db` — l'ancien `history.json` est importé automatiquement
- **Images** (captures d'écran, « Copier l'image »…) : vignette dans la liste, aperçu au survol, recopie en un clic ; PNG sans perte **chiffrés** dans `…/com.amine.copyq-rs/images/` (`.png.enc`), 50 images au plus par défaut (réglage **Enregistrer les images**)
- **Fichiers copiés dans le Finder** (vidéos, sons, PDF, dossiers…) : gardés par leur emplacement, recopiables pour les coller ailleurs ; signalés « introuvable » s'ils ont été déplacés
- **Limites réglables** (Réglages → onglet *Limites*) : nombre d'éléments (500 par défaut), d'images (50), taille d'un texte (1 000 Ko), taille totale des textes (200 Mo), d'une image (50 Mpx), durée de conservation (illimitée par défaut ; 1 jour à 1 an). Les changements s'appliquent avec **Appliquer**, qui indique d'abord combien d'éléments seront supprimés ; **Valeurs par défaut** restaure les réglages d'origine
- **11 langues** (Réglages → Général → *Langue*) : anglais, français, allemand, espagnol, italien, portugais (Brésil), japonais, coréen, chinois simplifié et traditionnel, arabe — les principaux marchés de l'App Store. En arabe, l'interface est en miroir (de droite à gauche) ; un texte copié en arabe s'affiche de droite à gauche quelle que soit la langue (police arabe du système en secours). Kit de relecture pour un traducteur : `docs/traduction-arabe.csv`. Par défaut, la langue de macOS (anglais si elle n'est pas traduite) ; polices système chinoises, japonaises et coréennes chargées au besoin. Textes dans `locales/*.json` ; des tests vérifient que chaque langue a toutes les clés et les mêmes paramètres
- Recherche instantanée (tous les mots, insensible à la casse)
- Clic ou **Entrée** : **collé directement** dans l'application d'où l'historique a été ouvert (⌘⇧Z) — nécessite l'autorisation *Accessibilité* (Réglages Système → Confidentialité et sécurité → Accessibilité) ; sans elle, ou si le réglage **Coller automatiquement** est désactivé, l'élément est copié et il reste à faire ⌘V
- Navigation **↑ / ↓**, **⌘⌫** supprime l'élément sélectionné, **Échap** vide la recherche puis masque
- **⌘⇧Z** (raccourci global) affiche l'historique, ou le masque s'il est déjà au premier plan
- Icône dans la barre des menus (Afficher l'historique / Quitter) ; fermer la fenêtre la masque seulement
- Thème moderne clair / sombre qui suit le réglage de macOS
- Interrupteur **Pause** pour ne pas enregistrer ce que vous copiez
- Panneau **Réglages** : masquer après copie, ouvrir au démarrage (nécessite l'app empaquetée `.app`), enregistrer les mots de passe — conservés dans `settings.json`

## Robustesse

- **Économe** : seul le compteur de changements du presse-papiers est consulté (toutes les 300 ms) ; le contenu n'est lu que lorsqu'il change
- **Mots de passe ignorés par défaut** : les copies marquées confidentielles ou temporaires (1Password, Bitwarden, trousseau… — convention nspasteboard.org) ne sont ni lues ni enregistrées. Réglable dans **Réglages → Enregistrer les mots de passe** : ils sont alors gardés, affichés masqués (••••) et exclus de la recherche ; un bouton permet de tous les effacer
- **Mots de passe d'extensions de navigateur** (Dashlane, 1Password, Bitwarden… : copiés sans marqueur) : réglage **Détecter les mots de passe** (activé par défaut) — un texte sans espace de 8 à 128 caractères mêlant majuscules, minuscules et chiffres est traité comme un mot de passe ; ⌘L ou le cadenas au survol marque/démarque un élément à la main ; les éléments déjà enregistrés sont rattrapés au lancement
- **Mots de passe chiffrés** : AES-256-GCM (nonce aléatoire par élément), clé de 256 bits générée au premier besoin et conservée dans le **trousseau macOS** (service `com.amine.copyq-rs`, compte `history-encryption-key`) ; doublons et suppressions via une empreinte HMAC-SHA256. Si le trousseau est inaccessible, un mot de passe n'est **jamais** écrit en clair (il reste en mémoire). Les mots de passe stockés en clair par la v0.2 sont chiffrés au lancement.
- **Images chiffrées** : images et vignettes chiffrées en AES-256-GCM avec une sous-clé de la même clé du trousseau, liées à leur nom (on ne peut pas en échanger deux). Sans accès au trousseau, une image n'est pas gardée ; les images en clair des versions précédentes sont chiffrées au lancement.
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
| `src/media.rs` | Images : PNG, vignettes, empreintes, fichiers chiffrés |
| `src/paste.rs` | Collage automatique (app d'origine, ⌘V simulé) |
| `src/i18n.rs`, `locales/` | Traductions (10 langues), détection de la langue de macOS |
| `src/crypto.rs` | Chiffrement AES-256-GCM (mots de passe, fichiers) et empreintes HMAC |
| `src/keychain.rs` | Clé de chiffrement dans le trousseau macOS |
| `src/app/mod.rs` | Interface egui : état, recherche, pied de page, raccourcis clavier |
| `src/app/row.rs` | Lignes de la liste : aperçu, icône, vignette, actions |
| `src/app/settings_panel.rs` | Panneau Réglages (Général, Limites) |
| `src/app/widgets.rs` | Petits composants (interrupteur, touches, légendes) |
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
