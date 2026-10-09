# PdfKub app icon

<img src="pdfkub.svg" alt="PdfKub app icon: a red panda peeking over a PDF page and holding it with both paws, on night blue" width="128">

**Creature:** a red panda (แพนด้าแดง), head and paws, peeking over the top edge of a PDF page and holding
it. Cream brows, cheeks and muzzle, dark tear marks under the eyes, cream-rimmed ears.

**Palette:**

| Colour | Hex | Used for |
|---|---|---|
| Night blue (app colour) | `#27477a` → `#121f3d` | the full-bleed field, top to bottom |
| Fur | `#e8692d` → `#bb4015` | head and ears, top to bottom |
| Dark fur | `#6b2410`, `#3a1a10` | tear marks and inner ears; paws |
| Cream | `#fff6ea` | brows, cheeks, muzzle, ear rims |
| Ink | `#1a0f0b` | eyes, nose, mouth |
| Paper | `#ffffff`, `#d9e2f2`, `#c3cfe6` | the page, its folded corner, its text lines |

**Tile:** `viewBox="0 0 512 512"`, a rounded square with `rx=112` that clips everything. Windows and Linux
icons use the full-bleed tile. macOS icons put it on Apple's grid (an 824 px body centred on a transparent
1024 px canvas).

**Small sizes:** `pdfkub-small.svg` (used at 24 px and below) drops the eye highlights, blush, mouth, claws
and thin text lines, and draws bigger eyes and two thick lines instead.

**Provenance:** drawn as plain SVG shapes by [`packaging/make_icon.py`](../../packaging/make_icon.py); no
fonts or third-party artwork. Licence: [LICENSE.txt](LICENSE.txt) (`MIT OR Apache-2.0`, like the repo).

## Files

| File | What it is |
|---|---|
| `pdfkub.svg` | the master vector; every PNG, `.ico` and `.icns` at 32 px and up is rendered from it |
| `pdfkub-small.svg` | the 16–24 px variant |
| `pdfkub-1024.png` | 1024 px on Apple's grid; also the runtime Dock icon on macOS |
| `pdfkub.icns` | macOS icon (16–1024 px) |
| `pdfkub.ico` | Windows icon (16–256 px), embedded in `pdfkub.exe` by `apps/pdfkub/build.rs` |
| `hicolor/<n>x<n>/apps/io.github.teh_natsu.pdfkub.png` | Linux hicolor theme, 16–512 px; the 256 px one is the runtime icon on Windows and Linux |
| `hicolor/scalable/apps/io.github.teh_natsu.pdfkub.svg` | Linux scalable icon (copy of the master) |

Where it shows: `apps/pdfkub/src/main.rs` sets the window icon (Dock, taskbar, Alt-Tab, launcher) and the
Wayland app id `io.github.teh_natsu.pdfkub`; `packaging/linux/io.github.teh_natsu.pdfkub.desktop` names the
hicolor icon; the About dialog and home screen show `pdfkub.svg`.

## Regenerate

```sh
python3 packaging/make_icon.py assets/app-icon   # writes pdfkub.svg and pdfkub-small.svg
packaging/icons.sh        # needs resvg and python3 (Pillow for the .icns off macOS)
cargo xtask assets        # then update the sha256 values in ATTRIBUTION.toml and run with --write
```
