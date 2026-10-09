#!/usr/bin/env bash
# Regenerate every app icon from assets/app-icon/pdfkub.svg (the master vector) and
# pdfkub-small.svg (the variant for 24 px and below). Both SVGs come from packaging/make_icon.py.
#
# Needs: resvg (brew install resvg / cargo install resvg) and python3 (stdlib for the .ico). The
# .icns is written by iconutil on macOS, or by Pillow elsewhere. The outputs are committed, so
# building and packaging never need these tools. After running it, update the sha256 values in
# ATTRIBUTION.toml, then `cargo xtask assets --write`.
#
#   packaging/icons.sh
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIR="$ROOT/assets/app-icon"
SVG="$DIR/pdfkub.svg"
SMALL="$DIR/pdfkub-small.svg"
ID="io.github.teh_natsu.pdfkub"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

command -v resvg >/dev/null || { echo "error: resvg not found (brew install resvg)" >&2; exit 1; }

# The masters are full-bleed 512 tiles (rx=112): right for Windows and Linux. macOS icons follow
# Apple's grid instead: an 824 px body centred on a transparent 1024 canvas.
mac() {
  # Git Bash on Windows: resvg is a Windows program and can't open /d/... paths.
  local href="$1"
  if command -v cygpath >/dev/null; then href="$(cygpath -m "$1")"; fi
  echo '<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 1024 1024">'
  echo '<image x="100" y="100" width="824" height="824" xlink:href="'"$href"'"/>'
  echo '</svg>'
}
MAC="$TMP/macos.svg"
MAC_SMALL="$TMP/macos-small.svg"
mac "$SVG" >"$MAC"
mac "$SMALL" >"$MAC_SMALL"

render() { resvg -w "$2" -h "$2" "$1" "$3" </dev/null; }
# The lettering can't be read at 24 px and below: use the small variant there.
pick() { if [ "$1" -le 24 ]; then echo "$SMALL"; else echo "$SVG"; fi; }
pick_mac() { if [ "$1" -le 32 ]; then echo "$MAC_SMALL"; else echo "$MAC"; fi; }

# 1024 px PNG on Apple's grid (also the runtime Dock icon on macOS, see apps/pdfkub/src/main.rs).
render "$MAC" 1024 "$DIR/pdfkub-1024.png"

# Linux hicolor theme (full bleed; hicolor/256x256 is also the runtime icon on Windows and Linux).
for s in 16 24 32 48 64 128 256 512; do
  mkdir -p "$DIR/hicolor/${s}x${s}/apps"
  render "$(pick "$s")" "$s" "$DIR/hicolor/${s}x${s}/apps/$ID.png"
done
mkdir -p "$DIR/hicolor/scalable/apps"
cp "$SVG" "$DIR/hicolor/scalable/apps/$ID.svg"

# Windows .ico: PNG-compressed entries, 16-256 px.
ICO_PNGS=()
for s in 16 20 24 32 40 48 64 128 256; do
  render "$(pick "$s")" "$s" "$TMP/ico-$s.png"
  ICO_PNGS+=("$TMP/ico-$s.png")
done
python3 - "$DIR/pdfkub.ico" "${ICO_PNGS[@]}" <<'PY'
import struct, sys
out, pngs = sys.argv[1], sys.argv[2:]
blobs = [open(p, "rb").read() for p in pngs]
head = struct.pack("<HHH", 0, 1, len(blobs))
entries, data, offset = b"", b"", 6 + 16 * len(blobs)
for b in blobs:
    w, h = struct.unpack(">II", b[16:24])  # IHDR
    entries += struct.pack("<BBBBHHII", w % 256, h % 256, 0, 0, 1, 32, len(b), offset)
    data += b
    offset += len(b)
open(out, "wb").write(head + entries + data)
PY

# macOS .icns.
if command -v iconutil >/dev/null; then
  SET="$TMP/pdfkub.iconset"
  mkdir -p "$SET"
  for s in 16 32 128 256 512; do
    render "$(pick_mac "$s")" "$s" "$SET/icon_${s}x${s}.png"
    render "$(pick_mac $((s * 2)))" $((s * 2)) "$SET/icon_${s}x${s}@2x.png"
  done
  iconutil -c icns -o "$DIR/pdfkub.icns" "$SET"
else
  for s in 16 32 64 128 256 512 1024; do
    render "$(pick_mac "$s")" "$s" "$TMP/mac-$s.png"
  done
  python3 - "$DIR/pdfkub.icns" "$TMP" <<'PY'
import sys
from PIL import Image
out, tmp = sys.argv[1], sys.argv[2]
others = [Image.open(f"{tmp}/mac-{s}.png") for s in (16, 32, 64, 128, 256, 512)]
Image.open(f"{tmp}/mac-1024.png").save(out, append_images=others)
PY
fi
echo "icons written to $DIR"
