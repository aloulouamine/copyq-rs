#!/bin/bash
# Empaquette un binaire en application macOS.
#
#   ci/make_app.sh <binaire> <dossier .app> [version]
#
# Signature : identité « Developer ID Application » si MACOS_SIGN_IDENTITY est
# défini (runtime renforcé, prêt pour la notarisation), sinon signature ad hoc.
set -euo pipefail
BIN="$1"
APP="$2"
VERSION="${3:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)}"
BUILD="${BUILD_NUMBER:-1}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/copyq-rs"
chmod +x "$APP/Contents/MacOS/copyq-rs"

# Icône : .icns construit depuis assets/icon-1024.png (sips et iconutil de macOS).
ICONSET="$(mktemp -d)/AppIcon.iconset"
mkdir -p "$ICONSET"
for size in 16 32 128 256 512; do
  sips -z $size $size "$ROOT/assets/icon-1024.png" --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2))
  sips -z $double $double "$ROOT/assets/icon-1024.png" --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/AppIcon.icns"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleIdentifier</key><string>com.amine.copyq-rs</string>
  <key>CFBundleName</key><string>CopyQ-rs</string>
  <key>CFBundleDisplayName</key><string>CopyQ-rs</string>
  <key>CFBundleExecutable</key><string>copyq-rs</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>${BUILD}</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.utilities</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSHumanReadableCopyright</key><string>© Amine Aloulou</string>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleLocalizations</key><array>
    <string>en</string><string>fr</string><string>de</string><string>es</string>
    <string>it</string><string>pt-BR</string><string>ja</string><string>ko</string>
    <string>zh-Hans</string><string>zh-Hant</string><string>ar</string>
  </array>
</dict></plist>
PLIST
plutil -lint "$APP/Contents/Info.plist" >/dev/null

if [ -n "${MACOS_SIGN_IDENTITY:-}" ]; then
  codesign --force --timestamp --options runtime --sign "$MACOS_SIGN_IDENTITY" "$APP"
else
  codesign --force --sign - "$APP"
fi
codesign --verify --strict "$APP"
echo "$APP ($VERSION, build $BUILD)"
