#!/bin/bash
# Empaquette le binaire dans une app .app minimale (signature ad hoc) et teste
# l'inscription au lancement à l'ouverture de session (SMAppService).
set -euo pipefail
APP="$RUNNER_TEMP/CopyQ-rs.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp target/release/copyq-rs "$APP/Contents/MacOS/copyq-rs"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleIdentifier</key><string>com.amine.copyq-rs</string>
  <key>CFBundleName</key><string>CopyQ-rs</string>
  <key>CFBundleExecutable</key><string>copyq-rs</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
</dict></plist>
PLIST
codesign --force --sign - "$APP"
"$APP/Contents/MacOS/copyq-rs" --login-item-selftest
