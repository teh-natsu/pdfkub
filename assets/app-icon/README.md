# PdfKub app icon

<img src="pdfkub.svg" alt="PdfKub app icon: a flat red panda resting its paws on a white card with a PDF page (folded corner and text bars), on blue glass" width="128">

**Creature:** a flat red panda (แพนด้าแดง), head and paws, resting its paws on a white card that shows
a PDF page (folded corner and text bars). Cream inner ears, brows, cheeks and muzzle, dark tear marks under the eyes. The same panda in all three
*Kub apps ([PdfKub](https://github.com/teh-natsu/pdfkub), [LightKub](https://github.com/teh-natsu/lightkub),
[CadKub](https://github.com/teh-natsu/cadkub)); each has its own tile colour and symbol.

**Palette:**

| Colour | Hex | Used for |
|---|---|---|
| Blue (app colour) | `#4c8dff` → `#1d48c9` | the tile, top-left to bottom-right; the symbol on the card |
| Glass lights | `#7fe3ff`, `#8a5cff` | two blurred lights behind the frosted pane (top-left, bottom-right) |
| Glass | white at 7 %, a white sheen and a white rim | the frosted pane, its diagonal sheen and its lit edge |
| Fur | `#f2732f`; paws `#d85a1e` | head and ears; paws |
| Cream | `#fff5e8` | inner ears, brows, cheeks, muzzle |
| Tear marks | `#a3391a` | under the eyes |
| Ink | `#22140f` | eyes, nose, mouth |
| Card | `#ffffff` | the page: folded corner and three text bars in the app colour |

**Tile:** `viewBox="0 0 512 512"`, a rounded square with `rx=114` that clips everything; the card casts a
soft shadow. Windows and Linux icons use the full-bleed tile. macOS icons put it on Apple's grid (an
824/1024 body with a transparent margin).

**Small sizes:** `pdfkub-small.svg` (used at 24 px and below) drops the glass lights, the shadow, the eye
highlights and the mouth, and draws bigger eyes, a thicker rim and two thick text bars.

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
