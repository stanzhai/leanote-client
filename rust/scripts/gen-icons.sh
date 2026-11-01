#!/bin/sh
# Regenerate the icon set from logo/icon-black-white.png (ADR-0011).
# Run on macOS. Tools: sips + iconutil (stock) + Pillow.
# Outputs are committed so CI never needs a graphics toolchain:
#   rust/packaging/icon.icns                 macOS .app (neutral glyph)
#   rust/packaging/icon.ico                  Windows exe embedding (win tile)
#   rust/packaging/icons/128x128.png …       Linux AppImage/deb hicolor
#   rust/crates/leanote-app/assets/app-icon.png  Windows/Linux titlebar (win tile)
#
# Source: 1690×1690 grayscale, WHITE glyph on BLACK (glyph bbox ~396..1418).
# macOS and Windows/Linux now share one look: a full-bleed LANCZOS downscale
# of the source (opaque black square, white ant glyph, smooth edges — user
# round 2026-11-01 rejected the earlier thresholded rounded tile as jagged).
# The Windows exe icon (.ico) and the in-app titlebar icon (app-icon.png) come
# from the same frames so both always match byte-for-byte at 128px.
set -eu

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
SRC="$ROOT/logo/icon-black-white.png"
OUT="$ROOT/rust/packaging"
TITLEBAR="$ROOT/rust/crates/leanote-app/assets"
WORK=$(mktemp -d /tmp/leanote-icons.XXXXXX)
trap 'rm -rf "$WORK"' EXIT

mkdir -p "$OUT/icons" "$TITLEBAR" "$WORK/mac" "$WORK/win"

# --- Derive the two glyph variants with Pillow -----------------------------
python3 - "$SRC" "$WORK" <<'PY'
import sys, os
from PIL import Image

src, work = sys.argv[1], sys.argv[2]
base = Image.open(src).convert("L")

# mac: neutral glyph, no tile (the system applies the app shape).
for s in (16, 32, 64, 128, 256, 512, 1024):
    base.resize((s, s), Image.LANCZOS).convert("RGBA").save(
        os.path.join(work, "mac", f"{s}.png"))

# win: full-bleed downscale of the source — opaque black square, white
# glyph, the composition exactly as drawn in the source (user round
# 2026-11-01: jaggies complaint). Anti-aliasing comes from LANCZOS over the
# 1690px master (~140 gray steps at 32px); any thresholding or small-size
# mask drawing re-introduces hard binary edges and was removed for that
# reason. Identical math to the mac variant; kept as a separate output so
# the two families can diverge again if the taskbar look changes.
for s in (16, 32, 48, 64, 128, 256):
    base.resize((s, s), Image.LANCZOS).convert("RGBA").save(
        os.path.join(work, "win", f"{s}.png"))
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

# --- titlebar asset (same win tile the exe icon uses) -----------------------
# The in-app Windows/Linux header icon must stay byte-identical to the exe
# icon's 128px entry; both come from the same $WORK/win/128.png.
cp "$WORK/win/128.png" "$TITLEBAR/app-icon.png"

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
