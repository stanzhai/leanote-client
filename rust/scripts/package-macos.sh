#!/bin/sh
# Package Leanote for macOS: arch-specific .app + .dmg.
#
# Usage: package-macos.sh <arch> [output-dir] [version]
#   arch    aarch64 | x86_64 (build target triple suffix)
#   version defaults to Cargo.toml workspace version
#
# Env override for CI: VERSION (3rd arg wins), used for release naming.
# The GUI exe is `Leanote`; the CLI ships as `leanote-cli` because the
# default APFS/NTFS volumes are case-insensitive (`Leanote` == `leanote`).
# ~/bin/leanote shim points at the packaged CLI.
set -eu

ARCH=${1:?usage: package-macos.sh <aarch64|x86_64> [output-dir] [version]}
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
OUT=${2:-"$ROOT/dist"}
VERSION=${3:-$(grep -A1 '^version' "$ROOT/rust/Cargo.toml" | grep -m1 '"' | sed 's/.*"\(.*\)".*/\1/')}
TARGET="$ARCH-apple-darwin"
APP_NAME=Leanote
BUNDLE_ID=site.stanzhai.leanote
APP="$OUT/$APP_NAME.app"
DMG="$OUT/Leanote_${VERSION}_macos_${ARCH}.dmg"

echo "==> cargo build --release --target $TARGET"
(cd "$ROOT/rust" && cargo build --release --workspace --target "$TARGET")

BIN_DIR="$ROOT/rust/target/$TARGET/release"
echo "==> assembling $APP"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN_DIR/Leanote"     "$APP/Contents/MacOS/$APP_NAME"
cp "$BIN_DIR/leanote-cli" "$APP/Contents/MacOS/leanote-cli"

# CI macOS runners have ~14GB free; the cargo target dir can exceed that,
# starving hdiutil (which needs scratch for the UDZO image). The binaries
# are already copied into the .app, so drop target/ before imaging. Locally
# the var is unset and nothing is deleted.
if [ -n "${LEANOTE_CI_CLEAN:-}" ]; then
  rm -rf "$ROOT/rust/target"
fi
cp "$ROOT/rust/packaging/icon.icns" "$APP/Contents/Resources/app.icns"

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

echo "==> $DMG"
rm -f "$DMG"
hdiutil create -volname "$APP_NAME" -srcfolder "$APP" -ov -format UDZO "$DMG" >/dev/null

# Local convenience only (not part of CI output): point the shim at the
# packaged CLI so `leanote` keeps working after target/ cleanups.
if [ -d "$HOME/bin" ] && [ "${LEANOTE_SKIP_SHIM:-0}" != "1" ]; then
  printf '#!/bin/sh\nexec "%s/Contents/MacOS/leanote-cli" "$@"\n' "$APP" > "$HOME/bin/leanote"
  chmod +x "$HOME/bin/leanote"
fi

echo "done:"
echo "  $APP"
echo "  $DMG"
