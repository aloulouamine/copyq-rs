# Relecture de la traduction arabe

Fichier à envoyer au traducteur : `traduction-arabe.csv` (s'ouvre dans Excel, Numbers
ou Google Sheets).

- Une ligne par texte de l'interface, puis les textes de la fiche App Store (`store.*`).
- Colonnes : contexte (où le texte apparaît), anglais et français (références),
  **arabe (proposition)** à relire, **arabe (relu)** à remplir seulement si la
  proposition doit changer, remarques, et points à vérifier.
- Les `{n}`, `{name}`, `{error}`, `{query}` doivent rester tels quels (ils sont remplacés
  par un nombre, un nom de fichier, un message…). Les tests de l'app le vérifient.
- Arabe standard moderne, vocabulaire de macOS en arabe (« إعدادات النظام »,
  « سلسلة المفاتيح », « تسهيلات الاستخدام »…), chiffres occidentaux (0–9).
- Points d'attention signalés dans la colonne « à vérifier » : accords avec les
  nombres (ملفان / ملفات / ملفًا…), noms exacts des menus de macOS, longueur des
  textes de la fiche App Store.

Réintégration du fichier relu :

```
python3 ci/import_traduction.py docs/traduction-arabe.csv ar
cargo test
```
