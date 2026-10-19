#!/bin/bash
# Image disque d'installation : l'app et un raccourci vers /Applications.
#
#   ci/make_dmg.sh <CopyQ-rs.app> <sortie.dmg>
set -euo pipefail
APP="$1"
DMG="$2"
STAGE="$(mktemp -d)/CopyQ-rs"
mkdir -p "$STAGE"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
rm -f "$DMG"
hdiutil create -volname "CopyQ-rs" -srcfolder "$STAGE" -fs HFS+ -format UDZO -imagekey zlib-level=9 -ov "$DMG" >/dev/null
if [ -n "${MACOS_SIGN_IDENTITY:-}" ]; then
  codesign --force --timestamp --sign "$MACOS_SIGN_IDENTITY" "$DMG"
fi
hdiutil verify "$DMG" >/dev/null
echo "$DMG ($(du -h "$DMG" | cut -f1))"
