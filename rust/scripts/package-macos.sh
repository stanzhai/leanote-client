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
BUNDLE_ID=site.stanzhai.leanote.client
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

# ADR-0011: drag-to-Install layout — read-write staging volume with an
# /Applications symlink, background PNG and icon positions applied via
# AppleScript, then flattened to a compressed UDZO. Pillow renders the
# background (a solid + arrow text); skip silently when unavailable
# (plain UDZO still installs fine, just without the layout).
STAGING="$OUT/.dmg-staging"
rm -rf "$STAGING"
mkdir -p "$STAGING"
cp -R "$APP" "$STAGING/"
ln -s /Applications "$STAGING/Applications"

BG_SRC="$ROOT/logo/icon-black-white.png"
BG="$STAGING/.background.png"
python3 - "$BG_SRC" "$BG" 660 <<'PY' 2>/dev/null || true
import sys
from PIL import Image, ImageDraw
src, dst, size = sys.argv[1], sys.argv[2], int(sys.argv[3])
bg = Image.new("RGB", (size, size), (245, 245, 248))
icon = Image.open(src).convert("RGBA")
icon.thumbnail((size // 3, size // 3))
bg.paste(icon, (size // 8, (size - icon.height) // 2), icon)
d = ImageDraw.Draw(bg)
d.text((size // 8 + icon.width + 24, size // 2 + 44), "拖到 Applications 安装",
       fill=(120, 120, 130))
bg.save(dst, "PNG")
PY
if [ -f "$BG" ]; then
  mkdir -p "$STAGING/.background"
  mv "$BG" "$STAGING/.background/background.png"
fi

hdiutil create -volname "$APP_NAME" -srcfolder "$STAGING" -ov -format UDZO "$DMG" >/dev/null
rm -rf "$STAGING"

# Re-apply icon positions + background on the finished image (Symlink and
# view options survive UDZO; positions need the AppleScript pass).
if command -v osascript >/dev/null 2>&1; then
  MOUNT=$(hdiutil attach -readwrite -nobrowse -mountpoint /tmp/leanote-dmg "$DMG" 2>/dev/null | head -1) || MOUNT=""
  if [ -n "$MOUNT" ]; then
    osascript <<OSA 2>/dev/null || true
tell application "Finder"
  tell disk "$APP_NAME"
    open
    set current view of container window to icon view
    set toolbar visible of container window to false
    set statusbar visible of container window to false
    set the bounds of container window to {0, 0, 660, 660}
    set arrangement of icon view options of container window to not arranged
    set icon size of icon view options of container window to 128
    set background picture of icon view options of container window to file ".background:background.png"
    set position of item "$APP_NAME" of container window to {180, 330}
    set position of item "Applications" of container window to {480, 330}
    update container window
    close
  end tell
end tell
OSA
    hdiutil detach /tmp/leanote-dmg >/dev/null 2>&1 || true
  fi
fi

# Local convenience only (not part of CI output): point the shim at the
# packaged CLI so `leanote` keeps working after target/ cleanups.
if [ -d "$HOME/bin" ] && [ "${LEANOTE_SKIP_SHIM:-0}" != "1" ]; then
  printf '#!/bin/sh\nexec "%s/Contents/MacOS/leanote-cli" "$@"\n' "$APP" > "$HOME/bin/leanote"
  chmod +x "$HOME/bin/leanote"
fi

echo "done:"
echo "  $APP"
echo "  $DMG"
