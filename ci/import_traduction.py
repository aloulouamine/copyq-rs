"""Réintègre une relecture : python3 ci/import_traduction.py docs/traduction-arabe.csv ar

Pour chaque clé de l'interface, prend la colonne « arabe (relu) » si elle est remplie,
sinon garde la proposition, et réécrit locales/<langue>.json (les lignes « store.* »,
textes de la fiche App Store, sont ignorées). Les tests vérifient ensuite les clés et
les paramètres {…}."""
import collections, csv, json, sys

path, lang = sys.argv[1], sys.argv[2]
out = collections.OrderedDict()
changed = 0
with open(path, encoding="utf-8-sig", newline="") as f:
    for row in csv.DictReader(f):
        key = row["clé"]
        if key.startswith("store."):
            continue
        reviewed = row.get("arabe (relu)", "").strip()
        out[key] = reviewed or row["arabe (proposition)"]
        changed += bool(reviewed)
with open(f"locales/{lang}.json", "w", encoding="utf-8") as f:
    json.dump(out, f, ensure_ascii=False, indent=2)
    f.write("\n")
print(f"{len(out)} textes écrits dans locales/{lang}.json, dont {changed} relus.")
