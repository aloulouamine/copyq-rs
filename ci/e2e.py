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
check("au redémarrage, la base est relue (13 éléments)", "base ouverte : 13 éléments" in restart_log)

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
    rows = stored_rows()
    encrypted = [t for t, sens in rows if sens and t.startswith("enc:v1:")]
    check("réglage activé : le mot de passe est enregistré chiffré", len(encrypted) >= 2)
    check("réglage activé : jamais écrit en clair", allowed.encode() not in db_bytes())
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

print(open("e2e-app.log").read())
sys.exit(1 if failures else 0)
