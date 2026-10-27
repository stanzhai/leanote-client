#!/bin/sh
# Package Leanote for Linux: AppImage + .deb, for the native arch.
#
# Usage: package-linux.sh <arch> [output-dir] [version]
#   arch    x86_64 | aarch64
#
# Requires on PATH: cargo, dpkg-deb, patchelf (AppImage AppDir fixup),
# and downloads appimagetool + AppRun from GitHub releases (cached in
# $ROOT/rust/target/appimagetool/).
set -eu

ARCH=${1:?usage: package-linux.sh <x86_64|aarch64> [output-dir] [version]}
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
OUT=${2:-"$ROOT/dist"}
VERSION=${3:-$(grep -A1 '^version' "$ROOT/rust/Cargo.toml" | grep -m1 '"' | sed 's/.*"\(.*\)".*/\1/')}
mkdir -p "$OUT"
PKG="$ROOT/rust/packaging"
CACHE="$ROOT/rust/target/appimagetool"
mkdir -p "$CACHE"

echo "==> cargo build --release"
(cd "$ROOT/rust" && cargo build --release --workspace)

BIN_DIR="$ROOT/rust/target/release"

# ---------- AppImage ----------
WORK=$(mktemp -d /tmp/leanote-appimage.XXXXXX)
trap 'rm -rf "$WORK"' EXIT
APPDIR="$WORK/Leanote.AppDir"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/metainfo" "$APPDIR/usr/share/icons/hicolor/512x512/apps"

cp "$BIN_DIR/Leanote" "$APPDIR/usr/bin/Leanote"
cp "$BIN_DIR/leanote-cli" "$APPDIR/usr/bin/leanote"
patchelf --set-rpath '$ORIGIN/../lib' "$APPDIR/usr/bin/Leanote" 2>/dev/null || true
cp "$PKG/icons/512x512.png" "$APPDIR/leanote.png"
cp "$PKG/icons/512x512.png" "$APPDIR/Leanote.png"

cat > "$APPDIR/leanote.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Leanote
Comment=Leanote note-taking desktop client
Exec=Leanote
Icon=leanote
Terminal=false
Categories=Office;
StartupWMClass=Leanote
EOF

cat > "$APPDIR/leanote.appdata.xml" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<component type="desktop-application">
  <id>site.stanzhai.leanote.client</id>
  <name>Leanote</name>
  <summary>Leanote note-taking desktop client</summary>
  <metadata_license>MIT</metadata_license>
  <launchable type="desktop-id">leanote.desktop</launchable>
</component>
EOF

if [ ! -x "$CACHE/appimagetool-$ARCH.AppImage" ]; then
  curl -fsSL -o "$CACHE/appimagetool-$ARCH.AppImage" \
    "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-${ARCH}.AppImage"
  chmod +x "$CACHE/appimagetool-$ARCH.AppImage"
fi
if [ ! -f "$CACHE/AppRun" ]; then
  curl -fsSL -o "$CACHE/AppRun" \
    "https://github.com/AppImage/AppImageKit/releases/download/continuous/AppRun-${ARCH}"
  chmod +x "$CACHE/AppRun"
fi
cp "$CACHE/AppRun" "$APPDIR/AppRun"

APPIMAGE="$OUT/Leanote_${VERSION}_linux_${ARCH}.AppImage"
rm -f "$APPIMAGE"
ARCH_TRIPLET="$ARCH" "$CACHE/appimagetool-$ARCH.AppImage" --no-appstream "$APPDIR" "$APPIMAGE" >/dev/null

# ---------- deb ----------
DEBROOT="$WORK/deb"
DEBDIR="$DEBROOT/usr"
mkdir -p "$DEBROOT/DEBIAN" "$DEBDIR/bin" "$DEBDIR/share/applications" \
         "$DEBDIR/share/icons/hicolor/512x512/apps" "$DEBDIR/share/metainfo"
cp "$BIN_DIR/Leanote" "$DEBDIR/bin/Leanote"
cp "$BIN_DIR/leanote-cli" "$DEBDIR/bin/leanote"
cp "$APPDIR/leanote.desktop" "$DEBDIR/share/applications/leanote.desktop"
cp "$PKG/icons/512x512.png" "$DEBDIR/share/icons/hicolor/512x512/apps/leanote.png"
cp "$APPDIR/leanote.appdata.xml" "$DEBDIR/share/metainfo/leanote.appdata.xml"

case "$ARCH" in
  x86_64)  DEB_ARCH=amd64 ;;
  aarch64) DEB_ARCH=arm64 ;;
  *)       echo "unknown arch $ARCH" >&2; exit 1 ;;
esac
# dpkg-deb rejects a Version that does not start with a digit; the tag's
# leading `v` stays in the artifact filename only.
DEB_VERSION="${VERSION#v}"
cat > "$DEBROOT/DEBIAN/control" <<EOF
Package: leanote
Version: $DEB_VERSION
Section: office
Priority: optional
Architecture: $DEB_ARCH
Depends: libxkbcommon0, libx11-6, libwayland-client0, libfontconfig1, libfreetype6
Maintainer: Stan Zhai <stan@stanzhai.site>
Description: Leanote note-taking desktop client
 Native desktop client for Leanote with local HTTP API, CLI and MCP server.
EOF

DEB="$OUT/Leanote_${VERSION}_linux_${ARCH}.deb"
rm -f "$DEB"
dpkg-deb --build --root-owner-group "$DEBROOT" "$DEB" >/dev/null

echo "done:"
echo "  $APPIMAGE"
echo "  $DEB"
