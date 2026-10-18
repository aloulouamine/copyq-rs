"""Tests de bout en bout sur un vrai Mac (CI) : lance l'app, copie du texte,
vérifie ce qui est enregistré. Usage : python3 ci/e2e.py <binaire>"""
import os, shutil, sqlite3, subprocess, sys, tempfile, time, uuid

BIN = sys.argv[1]
DATA = os.path.expanduser("~/Library/Application Support/com.amine.copyq-rs")
os.makedirs(DATA, exist_ok=True)


def stored_rows():
    """(texte, sensible) du plus récent au plus ancien."""
    src = os.path.join(DATA, "history.db")
    if not os.path.exists(src):
        return []
    snap = tempfile.mkdtemp()
    for suffix in ("", "-wal"):
        if os.path.exists(src + suffix):
            shutil.copy(src + suffix, os.path.join(snap, "h.db" + suffix))
    con = sqlite3.connect(os.path.join(snap, "h.db"))
    try:
        return [(r[0], bool(r[1])) for r in con.execute("SELECT text, sensitive FROM items ORDER BY id DESC")]
    finally:
        con.close()
        shutil.rmtree(snap, ignore_errors=True)


def db_bytes():
    out = b""
    for suffix in ("", "-wal", "-shm"):
        p = os.path.join(DATA, "history.db" + suffix)
        if os.path.exists(p):
            out += open(p, "rb").read()
    return out


def stored_kinds():
    """(nature, texte, meta) du plus récent au plus ancien."""
    src = os.path.join(DATA, "history.db")
    snap = tempfile.mkdtemp()
    for suffix in ("", "-wal"):
        if os.path.exists(src + suffix):
            shutil.copy(src + suffix, os.path.join(snap, "h.db" + suffix))
    con = sqlite3.connect(os.path.join(snap, "h.db"))
    try:
        return list(con.execute("SELECT kind, text, meta FROM items ORDER BY copied_at DESC, id DESC"))
    finally:
        con.close()
        shutil.rmtree(snap, ignore_errors=True)


def copy_png(path):
    subprocess.run(["osascript", "-e",
                    f'set the clipboard to (read (POSIX file "{path}") as «class PNGf»)'], check=True)


def copy_file(path):
    subprocess.run(["osascript", "-e", f'set the clipboard to (POSIX file "{path}")'], check=True)


def stored_texts():
    """Lit la base Turso avec le module sqlite3 de Python (vérifie la compatibilité
    du format). Turso verrouille le fichier pendant que l'app tourne : on lit une copie
    (base + journal WAL)."""
    src = os.path.join(DATA, "history.db")
    if not os.path.exists(src):
        return []
    snap = tempfile.mkdtemp()
    for suffix in ("", "-wal"):
        if os.path.exists(src + suffix):
            shutil.copy(src + suffix, os.path.join(snap, "h.db" + suffix))
    con = sqlite3.connect(os.path.join(snap, "h.db"))
    try:
        return [r[0] for r in con.execute("SELECT text FROM items ORDER BY id DESC")]
    finally:
        con.close()
        shutil.rmtree(snap, ignore_errors=True)


def pbcopy(text):
    subprocess.run(["pbcopy"], input=text.encode(), check=True)


def copy_concealed(text):
    js = f"""
    ObjC.import('AppKit');
    var pb = $.NSPasteboard.generalPasteboard;
    pb.clearContents;
    delay(0.8);  // laisse l'app voir le presse-papiers vide (cas réel de course)
    pb.setStringForType($('{text}'), $('public.utf8-plain-text'));
    pb.setStringForType($(''), $('org.nspasteboard.ConcealedType'));
    """
    subprocess.run(["osascript", "-l", "JavaScript", "-e", js], check=True)


failures = []


def check(name, cond):
    print(("OK   " if cond else "ÉCHEC"), name)
    if not cond:
        failures.append(name)


subprocess.run([sys.executable, "ci/seed_history.py", os.path.join(DATA, "history.json")], check=True)

# Image en clair laissée par une version précédente : doit être chiffrée au lancement.
IMG_DIR = os.path.join(DATA, "images")
os.makedirs(IMG_DIR, exist_ok=True)
LEGACY = os.path.join(IMG_DIR, "ancienne" + uuid.uuid4().hex[:8])
subprocess.run([sys.executable, "ci/make_png.py", LEGACY + ".png", "40", "20"], check=True)
PNG_MAGIC = b"\x89PNG"


def is_png(path):
    with open(path, "rb") as f:
        return f.read(4) == PNG_MAGIC

log = open("e2e-app.log", "w")
app = subprocess.Popen([BIN], stdout=log, stderr=subprocess.STDOUT)
time.sleep(5)
try:
    check("l'app tourne", app.poll() is None)

    texts = stored_texts()
    check("l'ancien history.json est importé dans la base (11 éléments)", len(texts) == 11)
    rows = stored_rows()
    check("le mot de passe importé est chiffré en base",
          any(sens and t.startswith("enc:v1:") for t, sens in rows))
    check("…et n'apparaît nulle part en clair dans les fichiers de la base",
          b"hunter2-MotDePasse!" not in db_bytes())
    check("l'ordre est conservé à l'import", bool(texts) and texts[0] == "https://github.com/emilk/egui")
    check("le JSON est renommé après import", os.path.exists(os.path.join(DATA, "history.json.migrated")))
    check("une image en clair d'une version précédente est chiffrée au lancement",
          not os.path.exists(LEGACY + ".png") and os.path.exists(LEGACY + ".png.enc")
          and not is_png(LEGACY + ".png.enc"))

    normal = f"texte-e2e-{uuid.uuid4().hex[:8]}"
    pbcopy(normal)
    time.sleep(1.5)
    check("une copie normale est enregistrée", normal in stored_texts())

    secret = f"secret-e2e-{uuid.uuid4().hex[:8]}"
    copy_concealed(secret)
    time.sleep(1.5)
    check("une copie confidentielle est ignorée", secret not in stored_texts())
    content_now = open(os.path.expanduser("~/Library/Logs/copyq-rs/copyq-rs.log")).read()
    check("…et détectée comme confidentielle malgré l'écriture en deux temps",
          content_now.count("Copie ignorée (contenu confidentiel)") >= 1)

    # Copie depuis une extension de navigateur (Dashlane…) : texte brut, sans marqueur.
    browser_pw = f"Dl#{uuid.uuid4().hex[:6]}Xk9!"
    pbcopy(browser_pw)
    time.sleep(1.5)
    check("mot de passe d'extension (non marqué) détecté et ignoré", browser_pw not in stored_texts())
    check("…et jamais écrit sur le disque", browser_pw.encode() not in db_bytes())

    # Image (comme une capture d'écran).
    png = os.path.join(tempfile.gettempdir(), "copyq-e2e.png")
    subprocess.run([sys.executable, "ci/make_png.py", png, "321", "123"], check=True)
    copy_png(png)
    time.sleep(2)
    rows = stored_kinds()
    images = [r for r in rows if r[0] == "image"]
    check("une image copiée est enregistrée", len(images) == 1)
    if images:
        _, h, meta = images[0]
        img_dir = os.path.join(DATA, "images")
        check("…avec ses dimensions", meta.startswith("321 × 123"))
        check("…et la taille du fichier, une fois encodé à part", " · " in meta)
        full = os.path.join(img_dir, h + ".png.enc")
        thumb = os.path.join(img_dir, h + ".thumb.png.enc")
        check("…son fichier et sa vignette", os.path.exists(full) and os.path.exists(thumb))
        check("…chiffrés (aucune image en clair sur le disque)",
              not is_png(full) and not is_png(thumb)
              and not any(n.endswith(".png") for n in os.listdir(img_dir)))
    copy_png(png)
    time.sleep(1.5)
    check("la même image recopiée n'est pas dupliquée",
          len([r for r in stored_kinds() if r[0] == "image"]) == 1)

    # Fichier copié dans le Finder (ex. une vidéo).
    media_file = os.path.join(tempfile.gettempdir(), "copyq e2e vidéo.mov")
    open(media_file, "wb").write(b"\0" * 1024)
    copy_file(media_file)
    time.sleep(2)
    files = [r for r in stored_kinds() if r[0] == "files"]
    check("un fichier copié dans le Finder est enregistré",
          len(files) == 1 and os.path.exists(files[0][1])
          and os.path.samefile(files[0][1], media_file))

    again = f"texte-e2e-{uuid.uuid4().hex[:8]}"
    pbcopy(again)
    time.sleep(1.5)
    texts = stored_texts()
    check("la copie suivante est bien enregistrée", again in texts)
    check("la plus récente est en tête", bool(texts) and texts[0] == again)
    # Instance unique : un 2e lancement doit se terminer aussitôt, sans tuer le 1er.
    second = subprocess.run([BIN], capture_output=True, text=True, timeout=20)
    check("un 2e lancement se termine proprement", second.returncode == 0)
    check("le 2e lancement prévient le 1er", "déjà lancé" in second.stderr)
    time.sleep(1)
    check("la 1re instance tourne toujours", app.poll() is None)

    logfile = os.path.expanduser("~/Library/Logs/copyq-rs/copyq-rs.log")
    content = open(logfile).read() if os.path.exists(logfile) else ""
    check("le journal fichier est écrit", "démarré" in content and "Copie ignorée" in content)
    check("la 1re instance a reçu la demande d'affichage", "nouveau lancement détecté" in content)
    check("binaire nu : lancement au démarrage signalé indisponible", "lancement au démarrage : Unavailable" in content)
finally:
    app.terminate()
    app.wait(timeout=10)
    log.close()

# Relance : les données doivent survivre au redémarrage.
app = subprocess.Popen([BIN], stdout=open("e2e-app2.log", "w"), stderr=subprocess.STDOUT)
time.sleep(4)
app.terminate()
app.wait(timeout=10)
restart_log = open("e2e-app2.log").read()
check("au redémarrage, la base est relue (15 éléments)", "base ouverte : 15 éléments" in restart_log)

# Réglage « Enregistrer les mots de passe » activé : la copie confidentielle est gardée, masquée.
settings_path = os.path.join(DATA, "settings.json")
with open(settings_path, "w") as f:
    f.write('{"save_passwords": true}')
app = subprocess.Popen([BIN], stdout=open("e2e-app3.log", "w"), stderr=subprocess.STDOUT)
time.sleep(4)
try:
    allowed = f"secret-autorise-{uuid.uuid4().hex[:8]}"
    copy_concealed(allowed)
    time.sleep(2)
    browser_kept = f"Dl#{uuid.uuid4().hex[:6]}Xk9!"
    pbcopy(browser_kept)
    time.sleep(2)
    rows = stored_rows()
    encrypted = [t for t, sens in rows if sens and t.startswith("enc:v1:")]
    check("réglage activé : les mots de passe sont enregistrés chiffrés", len(encrypted) >= 3)
    check("réglage activé : jamais écrits en clair",
          allowed.encode() not in db_bytes() and browser_kept.encode() not in db_bytes())
    log3 = open("e2e-app3.log").read()
    check("réglage relu au démarrage", "save_passwords: true" in log3)
    check("pas d'erreur de trousseau", "chiffrement indisponible" not in log3)

finally:
    app.terminate()
    app.wait(timeout=10)
    os.remove(settings_path)

# Relance : le mot de passe chiffré est relu et déchiffré (clé du trousseau).
app = subprocess.Popen([BIN], stdout=open("e2e-app4.log", "w"), stderr=subprocess.STDOUT)
time.sleep(4)
app.terminate()
app.wait(timeout=10)
log4 = open("e2e-app4.log").read()
check("au redémarrage, les mots de passe chiffrés sont déchiffrés",
      "non déchiffrable" not in log4 and "chiffrement indisponible" not in log4)

# Limites réglables : 12 éléments au plus → l'historique est réduit au lancement.
with open(settings_path, "w") as f:
    f.write('{"max_items": 12}')
before = len(stored_kinds())
app = subprocess.Popen([BIN], stdout=open("e2e-app5.log", "w"), stderr=subprocess.STDOUT)
time.sleep(4)
app.terminate()
app.wait(timeout=10)
os.remove(settings_path)
log5 = open("e2e-app5.log").read()
after = len(stored_kinds())
check(f"limite de 12 éléments appliquée au lancement ({before} → {after})",
      before > 12 and after == 12 and f"{before - 12} élément(s) supprimé(s)" in log5)

print(open("e2e-app.log").read())
sys.exit(1 if failures else 0)
