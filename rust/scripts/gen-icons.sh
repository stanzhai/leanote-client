#!/bin/sh
# Regenerate the icon set from logo/icon-black-white.png (ADR-0011).
# Run on macOS. Tools: sips + iconutil (stock) + rsvg-convert + Pillow.
# Outputs are committed so CI never needs a graphics toolchain:
#   rust/packaging/icon.icns        macOS .app (squircle, neutral glyph)
#   rust/packaging/icon.ico         Windows exe embedding (black bg, white glyph)
#   rust/packaging/icons/128x128.png …  Linux AppImage/deb hicolor
#
# Source: 1690×1690 8-bit grayscale (black glyph on white). macOS gets the
# neutral black-on-transparent; Windows gets white-on-black inside a rounded
# rect (Windows icons render on light taskbars).
set -eu

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
SRC="$ROOT/logo/icon-black-white.png"
OUT="$ROOT/rust/packaging"
WORK=$(mktemp -d /tmp/leanote-icons.XXXXXX)
trap 'rm -rf "$WORK"' EXIT

mkdir -p "$OUT/icons" "$WORK/mac" "$WORK/win"

# --- Derive the two glyph variants with Pillow -----------------------------
python3 - "$SRC" "$WORK" <<'PY'
import sys, os
from PIL import Image, ImageDraw, ImageOps

src, work = sys.argv[1], sys.argv[2]
base = Image.open(src).convert("L")

def glyph(size):
    g = base.resize((size, size), Image.LANCZOS)
    return g

# mac: black glyph on transparent, no padding (system applies the shape).
for s in (16, 32, 64, 128, 256, 512, 1024):
    glyph(s).convert("RGBA").save(os.path.join(work, "mac", f"{s}.png"))

# win: white glyph on black rounded rect (22% margin, 22% corner radius).
for s in (16, 32, 48, 64, 128, 256):
    margin = int(s * 0.22)
    radius = int(s * 0.22)
    canvas = Image.new("RGBA", (s, s), (0, 0, 0, 255))
    mask = Image.new("L", (s, s), 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, s - 1, s - 1], radius=radius, fill=255)
    black = Image.new("RGBA", (s, s), (0, 0, 0, 255))
    canvas = Image.composite(black, Image.new("RGBA", (s, s), (0, 0, 0, 0)), mask)
    g = ImageOps.invert(base.resize((s - 2 * margin, s - 2 * margin), Image.LANCZOS))
    white = Image.new("RGBA", g.size, (255, 255, 255, 255))
    white.putalpha(g)
    canvas.alpha_composite(white, (margin, margin))
    canvas.save(os.path.join(work, "win", f"{s}.png"))
PY

# --- Linux hicolor (neutral set) --------------------------------------------
for size in 16 32 64 128 256 512; do
  cp "$WORK/mac/$size.png" "$OUT/icons/${size}x${size}.png"
done

# --- .icns -------------------------------------------------------------------
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

# --- .ico (PNG-embedded multi-size container) -------------------------------
sizes="16 32 48 64 128 256"
python3 - "$OUT/icon.ico" "$WORK/win" $sizes <<'PY'
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
