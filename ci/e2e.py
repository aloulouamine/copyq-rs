"""Tests de bout en bout sur un vrai Mac (CI) : lance l'app, copie du texte,
vérifie ce qui est enregistré. Usage : python3 ci/e2e.py <binaire>"""
import json, os, subprocess, sys, time, uuid

BIN = sys.argv[1]
DATA = os.path.expanduser("~/Library/Application Support/com.amine.copyq-rs")
os.makedirs(DATA, exist_ok=True)


def stored_texts():
    path = os.path.join(DATA, "history.json")
    if not os.path.exists(path):
        return []
    with open(path) as f:
        return [it["text"] for it in json.load(f)["items"]]


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


log = open("e2e-app.log", "w")
app = subprocess.Popen([BIN], stdout=log, stderr=subprocess.STDOUT)
time.sleep(5)
try:
    check("l'app tourne", app.poll() is None)

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
finally:
    app.terminate()
    app.wait(timeout=10)
    log.close()

print(open("e2e-app.log").read())
sys.exit(1 if failures else 0)
