#!/bin/bash
# Test du collage automatique : une petite app cible au premier plan, puis
# `copyq-rs --paste-selftest` qui doit y coller un texte via ⌘V.
set -u
BIN="$(pwd)/target/release/copyq-rs"

# Autorisation Accessibilité (base TCC système ; possible seulement si la CI le permet).
for client in "$BIN" /bin/bash /usr/bin/python3; do
  sudo sqlite3 "/Library/Application Support/com.apple.TCC/TCC.db" \
    "INSERT OR REPLACE INTO access (service, client, client_type, auth_value, auth_reason, auth_version, indirect_object_identifier, flags) VALUES ('kTCCServiceAccessibility', '$client', 1, 2, 4, 1, 'UNUSED', 0);" \
    2>&1 || echo "TCC non modifiable pour $client"
done

swiftc -O ci/paste_target.swift -o /tmp/paste_target 2>&1 | tail -3
/tmp/paste_target &
TARGET=$!
sleep 4

TOKEN="colle-auto-$RANDOM"
"$BIN" --paste-selftest "$TOKEN"
rc=$?
sleep 1
got="$(cat /tmp/pasted.txt 2>/dev/null)"
kill $TARGET 2>/dev/null
echo "champ de la cible : « $got »"
if [ $rc -eq 2 ]; then
  echo "IGNORÉ : pas d'autorisation Accessibilité sur cette machine"
  exit 0
fi
if [[ "$got" == *"$TOKEN"* ]]; then echo "SUCCÈS"; else echo "ÉCHEC"; exit 1; fi
