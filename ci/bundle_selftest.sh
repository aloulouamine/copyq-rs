#!/bin/bash
# Empaquette le binaire en app .app (même script que les releases, signature
# ad hoc) et teste l'inscription au lancement à l'ouverture de session.
set -euo pipefail
APP="$RUNNER_TEMP/CopyQ-rs.app"
bash ci/make_app.sh target/release/copyq-rs "$APP"
test -f "$APP/Contents/Resources/AppIcon.icns"
plutil -extract CFBundleLocalizations raw -o - "$APP/Contents/Info.plist" | grep -q 11 \
  || { echo "CFBundleLocalizations : 11 langues attendues"; exit 1; }
"$APP/Contents/MacOS/copyq-rs" --login-item-selftest
