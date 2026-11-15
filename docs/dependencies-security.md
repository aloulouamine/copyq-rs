# Dépendances et contrôle de sécurité

`Cargo.lock` est versionné pour cette application. Utiliser `cargo build --locked`,
`cargo test --locked` et `cargo clippy --locked --all-targets -- -D warnings` sur
macOS. La release compile les cibles `aarch64-apple-darwin` et
`x86_64-apple-darwin` avec le même lockfile.

La résolution du 15 novembre 2026 comprend les correctifs suivants :

| Dépendance | Version verrouillée | Minimum corrigé | Avis |
|---|---|---|---|
| ctutils | 0.4.8 | 0.4.4 | [RUSTSEC-2026-0385](https://rustsec.org/advisories/RUSTSEC-2026-0385.html) |
| tokio | 1.54.1 | 1.54.1 | [RUSTSEC-2026-0384](https://rustsec.org/advisories/RUSTSEC-2026-0384.html) |
| rustix 1.x | 1.1.6 | 1.1.6 | [RUSTSEC-2026-0486](https://rustsec.org/advisories/RUSTSEC-2026-0486.html) |

La branche `rustix 0.38` n'est pas concernée par ce dernier avis.

## Audit automatisé

Le workflow `audit.yml` fonctionne sur une image macOS 15 supportée, chaque lundi
à 06:23 UTC, manuellement et sur les changements de manifestes/lockfiles. Il
utilise cargo-audit 0.23.0 avec une base RustSec actualisée. **Une vulnérabilité
sur n'importe quel paquet du lockfile fait échouer le job**, y compris si le
paquet n'est pas utilisé sur macOS. Les journaux restent accessibles pendant
14 jours dans l'artefact `dependency-audit-<run_id>-<run_attempt>`, même après un
échec ; ce workflow ne publie plus une branche `ci-audit`.

Les avertissements de RustSec (`unsound`, non maintenu, yanked) restent affichés,
sans liste d'avis ignorés. **Un avertissement sur un paquet présent dans un graphe
macOS (Apple Silicon ou Intel, dépendances normales et de compilation) fait aussi
échouer le job** : par exemple un retour de rustix 1.x sous 1.1.6
(RUSTSEC-2026-0486, `unsound`). Le détail figure dans `audit-macos-warnings.log`.
Les avertissements hors macOS restent informatifs. Les six avertissements
ci-dessous sont connus dans ce lockfile et absents des graphes macOS Apple
Silicon et Intel (dépendances normales **et** de compilation) :

| Avis | Paquet | Chemin hors macOS |
|---|---|---|
| [RUSTSEC-2024-0429](https://rustsec.org/advisories/RUSTSEC-2024-0429.html), `unsound` | glib 0.18.5 | tray-icon → libappindicator → glib (GTK/Linux) |
| [RUSTSEC-2024-0370](https://rustsec.org/advisories/RUSTSEC-2024-0370.html), non maintenu | proc-macro-error 1.0.4 | glib → glib-macros → proc-macro-error |
| [RUSTSEC-2026-0192](https://rustsec.org/advisories/RUSTSEC-2026-0192.html), non maintenu | ttf-parser 0.25.1 | eframe → winit → sctk-adwaita → ab_glyph → owned_ttf_parser |
| [RUSTSEC-2026-0370](https://rustsec.org/advisories/RUSTSEC-2026-0370.html), non maintenu | zvariant 5.15.1 | eframe → egui-winit → accesskit_winit → accesskit_unix → accesskit_atspi_common |
| [RUSTSEC-2026-0371](https://rustsec.org/advisories/RUSTSEC-2026-0371.html), non maintenu | zbus_names 4.3.5 | accesskit_unix → zbus → zbus_names |
| [RUSTSEC-2026-0372](https://rustsec.org/advisories/RUSTSEC-2026-0372.html), non maintenu | zvariant_derive 5.15.1 | zvariant → zvariant_derive |

Cette qualification doit être revue si les fonctionnalités, plateformes ou
versions changent. Les graphes `macos-*.log` joints à chaque audit permettent
cette vérification. Si Linux devient une cible prise en charge, prévoir la
mise à niveau de glib et les migrations GTK/Wayland/D-Bus chez les dépendances
parentes ; il ne s'agit pas de correctifs nécessaires au binaire macOS actuel.

Pour actualiser les dépendances, lancer `cargo update`, examiner le diff de
`Cargo.lock`, puis exécuter les tests, les compilations et `cargo audit` avant
de committer le lockfile. Mettre à jour ce document si les avertissements ou
leur applicabilité changent. Ne pas contourner `--locked` en CI.

## Outils de CI

Les actions checkout v7.0.1, cache v6.1.0 et upload-artifact v7.0.2 sont épinglées
par SHA et déclarent Node 24. Leurs entrées utilisées restent compatibles :
checkout conserve le checkout standard pour `push` et `pull_request`, cache
conserve `path`/`key`/`restore-keys`, et upload-artifact conserve explicitement
`archive: true` pour le DMG accompagné de son empreinte et pour les journaux.
La nouvelle option d'upload direct d'un seul fichier n'est pas activée.

Pillow est figé à 13.0.0 dans `ci/requirements-screenshots.txt`. Pour composer les
captures localement :

```sh
python3 -m pip install -r ci/requirements-screenshots.txt
python3 ci/readme_shots.py <shots> docs/screenshots
```

Les tests unitaires travaillent sur des données synthétiques et des répertoires
temporaires. Réserver les auto-tests de l'application, les captures et les tests
de bout en bout à un runner jetable : ils utilisent le presse-papiers et peuvent
modifier l'historique local.
