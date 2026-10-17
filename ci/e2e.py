"""Tests de bout en bout sur un vrai Mac (CI) : lance l'app, copie du texte,
vérifie ce qui est enregistré. Usage : python3 ci/e2e.py <binaire>"""
import os, sqlite3, subprocess, sys, time, uuid

BIN = sys.argv[1]
DATA = os.path.expanduser("~/Library/Application Support/com.amine.copyq-rs")
os.makedirs(DATA, exist_ok=True)


def stored_texts():
    """Lit la base Turso avec le module sqlite3 de Python (vérifie la compatibilité)."""
    path = os.path.join(DATA, "history.db")
    if not os.path.exists(path):
        return []
    con = sqlite3.connect(f"file:{path}?mode=ro", uri=True, timeout=5)
    try:
        return [r[0] for r in con.execute("SELECT text FROM items ORDER BY id DESC")]
    finally:
        con.close()


def pbcopy(text):
    subprocess.run(["pbcopy"], input=text.encode(), check=True)


def copy_concealed(text):
    js = f"""
    ObjC.import('AppKit');
    var pb = $.NSPasteboard.generalPasteboard;
    pb.clearContents;
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
    check("l'ancien history.json est importé dans la base (10 éléments)", len(texts) == 10)
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

    again = f"texte-e2e-{uuid.uuid4().hex[:8]}"
    pbcopy(again)
    time.sleep(1.5)
    texts = stored_texts()
    check("la copie suivante est bien enregistrée", again in texts)
    check("la plus récente est en tête", bool(texts) and texts[0] == again)
    logfile = os.path.expanduser("~/Library/Logs/copyq-rs/copyq-rs.log")
    content = open(logfile).read() if os.path.exists(logfile) else ""
    check("le journal fichier est écrit", "démarré" in content and "Copie ignorée" in content)
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
check("au redémarrage, la base est relue (12 éléments)", "base ouverte : 12 éléments" in restart_log)

print(open("e2e-app.log").read())
sys.exit(1 if failures else 0)
