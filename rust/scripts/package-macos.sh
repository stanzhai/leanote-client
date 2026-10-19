#!/bin/sh
# Package leanote-rust for macOS (ticket 16): .app bundle + .dmg.
#
# Usage: rust/scripts/package-macos.sh [output-dir]
#   - builds the release binaries (leanote CLI + leanote-app GUI)
#   - assembles Leanote.app (Info.plist + icon.icns + both binaries)
#   - writes a .dmg via hdiutil
#
# The CLI binary lives inside the .app so `~/bin/leanote` (rewritten by the
# first CLI run, or manually below) points at the packaged binary and keeps
# working when target/ is cleaned.

set -eu

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
OUT=${1:-"$ROOT/dist"}
APP_NAME=Leanote
BUNDLE_ID=site.stanzhai.leanote-rust
APP="$OUT/$APP_NAME.app"
VERSION=$(grep -m1 '^version' "$ROOT/rust/Cargo.toml" | sed 's/.*"\(.*\)".*/\1/')

echo "==> cargo build --release"
(cd "$ROOT/rust" && cargo build --release --workspace)

echo "==> assembling $APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$ROOT/rust/target/release/leanote-app" "$APP/Contents/MacOS/$APP_NAME"
cp "$ROOT/rust/target/release/leanote" "$APP/Contents/MacOS/leanote"
cp "$ROOT/icon.icns" "$APP/Contents/Resources/app.icns"

cat > "$APP/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key><string>$APP_NAME</string>
    <key>CFBundleIdentifier</key><string>$BUNDLE_ID</string>
    <key>CFBundleName</key><string>$APP_NAME</string>
    <key>CFBundleDisplayName</key><string>$APP_NAME</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundleVersion</key><string>$VERSION</string>
    <key>CFBundleIconFile</key><string>app.icns</string>
    <key>LSMinimumSystemVersion</key><string>12.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
</dict>
</plist>
EOF

# Gatekeeper-friendly quarantine-free local build: ad-hoc codesign.
codesign --force --deep --sign - "$APP" >/dev/null 2>&1 || echo "    (codesign skipped)"

echo "==> $OUT/$APP_NAME.dmg"
DMG="$OUT/$APP_NAME.dmg"
rm -f "$DMG"
hdiutil create -volname "$APP_NAME" -srcfolder "$APP" -ov -format UDZO "$DMG" >/dev/null

echo "==> pointing ~/bin/leanote at the packaged binary"
SHIM="$HOME/bin/leanote"
mkdir -p "$HOME/bin"
printf '#!/bin/sh\nexec "%s/Contents/MacOS/leanote" "$@"\n' "$APP" > "$SHIM"
chmod +x "$SHIM"

echo "done:"
echo "  $APP"
echo "  $DMG"
echo "  $SHIM -> $APP/Contents/MacOS/leanote"
