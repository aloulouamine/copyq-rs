"""Crée un historique de démonstration pour les captures d'écran CI."""
import json, os, sys, time

now = int(time.time())
samples = [
    ("https://github.com/emilk/egui", 20),
    ("Réunion d'équipe déplacée à 15h — salle Lumière", 140),
    ("fn main() {\n    println!(\"Bonjour depuis CopyQ-rs\");\n}\n", 600),
    ("amine@example.com", 1_900),
    ("~/Project/copyq/src/app.rs", 4_000),
    ("Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.", 9_000),
    ("SELECT id, name, created_at\nFROM users\nWHERE active = true\nORDER BY created_at DESC\nLIMIT 50;", 30_000),
    ("4 rue de la Paix, 75002 Paris", 90_000),
    ("cargo run --release", 200_000),
    ("Merci pour ton retour, je regarde ça demain matin !", 400_000),
]
items = [{"text": t, "copied_at": now - age} for t, age in samples]
path = sys.argv[1]
os.makedirs(os.path.dirname(path), exist_ok=True)
# Repartir d'une base vide : le JSON sera importé au lancement (test de migration).
import glob
for f in glob.glob(os.path.join(os.path.dirname(path), "history.db*")) + glob.glob(path + ".migrated"):
    os.remove(f)
with open(path, "w") as f:
    json.dump({"version": 1, "items": items}, f, ensure_ascii=False)
print(f"{len(items)} éléments écrits dans {path}")
