#!/bin/sh
# Regenerate the icon set from the master build/icon.png (Electron-era icon).
# Run on macOS (uses sips + iconutil, both stock). Outputs are committed so
# CI never needs a graphics toolchain:
#   rust/packaging/icon.icns        macOS .app
#   rust/packaging/icon.ico         Windows exe embedding (multi-size)
#   rust/packaging/icons/128x128.png …  Linux AppImage/deb hicolor
set -eu

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
SRC="$ROOT/logo.png"
OUT="$ROOT/rust/packaging"
WORK=$(mktemp -d /tmp/leanote-icons.XXXXXX)
trap 'rm -rf "$WORK"' EXIT

mkdir -p "$OUT/icons"
for size in 16 32 64 128 256 512; do
  sips -z $size $size "$SRC" --out "$OUT/icons/${size}x${size}.png" >/dev/null
done

# .icns: iconutil wants icon_16x16.png etc. inside an iconset dir.
ICONSET="$WORK/app.iconset"
mkdir -p "$ICONSET"
cp "$OUT/icons/16x16.png"   "$ICONSET/icon_16x16.png"
cp "$OUT/icons/32x32.png"   "$ICONSET/icon_16x16@2x.png"
cp "$OUT/icons/32x32.png"   "$ICONSET/icon_32x32.png"
cp "$OUT/icons/64x64.png"   "$ICONSET/icon_32x32@2x.png"
cp "$OUT/icons/128x128.png" "$ICONSET/icon_128x128.png"
cp "$OUT/icons/256x256.png" "$ICONSET/icon_128x128@2x.png"
cp "$OUT/icons/256x256.png" "$ICONSET/icon_256x256.png"
cp "$OUT/icons/512x512.png" "$ICONSET/icon_256x256@2x.png"
cp "$OUT/icons/512x512.png" "$ICONSET/icon_512x512.png"
iconutil -c icns "$ICONSET" -o "$OUT/icon.icns"

# .ico: PNG-embedded multi-size ico (modern Windows is fine with PNG frames
# above 256px; sizes <=256 use true BMP frames via sips' ico writer — sips
# can't do that, so assemble from the PNGs with a minimal ico container).
# ICO container: 6 entries, each pointing at a PNG frame.
sizes="16 32 48 64 128 256"
for size in $sizes; do
  if [ "$size" = 48 ]; then
    sips -z 48 48 "$SRC" --out "$WORK/$size.png" >/dev/null
  else
    cp "$OUT/icons/${size}x${size}.png" "$WORK/$size.png"
  fi
done
python3 - "$OUT/icon.ico" "$WORK" $sizes <<'PY'
import struct, sys, os
out, work, sizes = sys.argv[1], sys.argv[2], [int(s) for s in sys.argv[3:]]
entries = []
for s in sizes:
    data = open(os.path.join(work, f"{s}.png"), "rb").read()
    w = 0 if s >= 256 else s
    entries.append((w, len(data), data))
header = struct.pack("<HHH", 0, 1, len(entries))
off = 6 + 16 * len(entries)
dir_b = b""
img_b = b""
for w, ln, data in entries:
    dir_b += struct.pack("<BBBBHHII", w, w, 0, 0, 1, 32, ln, off + len(img_b))
    img_b += data
open(out, "wb").write(header + dir_b + img_b)
PY

echo "wrote: $OUT/icon.icns $OUT/icon.ico $OUT/icons/*.png"
