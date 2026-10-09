"""Build the PdfKub app icon SVGs: the master (pdfkub.svg) and the small-size variant (pdfkub-small.svg).

A red panda peeks over the top of a PDF page and holds it with both paws, on a night-blue tile.
Everything is plain SVG shapes, so no font or external artwork is needed.

  python3 packaging/make_icon.py assets/app-icon
"""
import sys

OUT_DIR = sys.argv[1]

NIGHT_TOP, NIGHT_BOTTOM = "#27477a", "#121f3d"
FUR_TOP, FUR_BOTTOM = "#e8692d", "#bb4015"
FUR_DARK = "#6b2410"  # tear marks, inner ears
PAW = "#3a1a10"
CREAM = "#fff6ea"
INK = "#1a0f0b"
PAPER, FOLD, LINE = "#ffffff", "#d9e2f2", "#c3cfe6"
BLUSH = "#ff9d7a"


def icon(small):
    p = [
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">',
        "<defs>",
        f'<linearGradient id="bg" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{NIGHT_TOP}"/>'
        f'<stop offset="1" stop-color="{NIGHT_BOTTOM}"/></linearGradient>',
        f'<linearGradient id="fur" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{FUR_TOP}"/>'
        f'<stop offset="1" stop-color="{FUR_BOTTOM}"/></linearGradient>',
        '<clipPath id="tile"><rect width="512" height="512" rx="112"/></clipPath>',
        "</defs>",
        '<g clip-path="url(#tile)">',
        '<rect width="512" height="512" fill="url(#bg)"/>',
    ]
    # The page: runs off the bottom of the tile, folded corner at the top right.
    page = "M134 300 H336 L384 348 V540 H134 Z"
    p += [
        f'<path d="{page}" transform="translate(0 10)" fill="#0a1228" opacity="0.45"/>',
        f'<path d="{page}" fill="{PAPER}"/>',
        f'<path d="M336 300 V334 Q336 348 350 348 H384 Z" fill="{FOLD}"/>',
    ]
    if small:
        p.append(f'<path d="M168 392 H316 M168 440 H276" stroke="{LINE}" stroke-width="26" stroke-linecap="round"/>')
    else:
        for y, x1 in ((380, 340), (412, 320), (444, 290), (476, 330)):
            p.append(f'<path d="M168 {y} H{x1}" stroke="{LINE}" stroke-width="14" stroke-linecap="round"/>')

    # Ears: fur, a cream rim and a dark inside.
    for sx in (1, -1):
        t = "" if sx == 1 else ' transform="translate(512 0) scale(-1 1)"'
        p += [
            f'<g{t}>',
            f'<path d="M104 182 Q92 82 150 62 Q208 70 218 130 Z" fill="{CREAM}"/>',
            f'<path d="M114 172 Q106 92 152 76 Q198 84 206 132 Z" fill="url(#fur)"/>',
            f'<path d="M132 150 Q128 104 154 94 Q182 102 186 134 Z" fill="{FUR_DARK}"/>',
            "</g>",
        ]
    # Head.
    p.append('<path d="M256 96 C356 96 420 150 420 222 C420 290 350 330 256 330 C162 330 92 290 92 222 C92 150 156 96 256 96 Z" fill="url(#fur)"/>')
    # Cream markings: brows, cheeks and muzzle.
    p += [
        f'<ellipse cx="200" cy="168" rx="26" ry="15" fill="{CREAM}" transform="rotate(-12 200 168)"/>',
        f'<ellipse cx="312" cy="168" rx="26" ry="15" fill="{CREAM}" transform="rotate(12 312 168)"/>',
        f'<path d="M100 236 C112 202 160 200 196 226 C214 258 196 300 150 300 C118 296 98 272 100 236 Z" fill="{CREAM}"/>',
        f'<path d="M412 236 C400 202 352 200 316 226 C298 258 316 300 362 300 C394 296 414 272 412 236 Z" fill="{CREAM}"/>',
        f'<path d="M256 214 C300 214 322 240 318 272 C312 306 284 322 256 322 C228 322 200 306 194 272 C190 240 212 214 256 214 Z" fill="{CREAM}"/>',
    ]
    # Tear marks from the eyes down past the muzzle.
    p += [
        f'<path d="M196 196 C214 196 228 214 226 240 C224 266 214 292 206 312 C194 290 186 262 186 236 C186 214 188 198 196 196 Z" fill="{FUR_DARK}"/>',
        f'<path d="M316 196 C298 196 284 214 286 240 C288 266 298 292 306 312 C318 290 326 262 326 236 C326 214 324 198 316 196 Z" fill="{FUR_DARK}"/>',
    ]
    # Eyes, nose and mouth.
    eye_r = 19 if small else 16
    p += [
        f'<circle cx="208" cy="214" r="{eye_r}" fill="{INK}"/>',
        f'<circle cx="304" cy="214" r="{eye_r}" fill="{INK}"/>',
    ]
    if not small:
        p += [
            '<circle cx="214" cy="208" r="5.5" fill="#ffffff"/>',
            '<circle cx="310" cy="208" r="5.5" fill="#ffffff"/>',
            f'<ellipse cx="168" cy="262" rx="16" ry="9" fill="{BLUSH}" opacity="0.55"/>',
            f'<ellipse cx="344" cy="262" rx="16" ry="9" fill="{BLUSH}" opacity="0.55"/>',
        ]
    p.append(f'<path d="M234 248 Q256 240 278 248 Q276 266 256 274 Q236 266 234 248 Z" fill="{INK}"/>')
    if not small:
        p.append(f'<path d="M256 274 V286 M240 292 Q256 302 272 292" stroke="{INK}" stroke-width="5" fill="none" stroke-linecap="round"/>')
    # Paws over the top edge of the page.
    for cx in (186, 326):
        p.append(f'<path d="M{cx - 34} 318 Q{cx - 36} 286 {cx} 284 Q{cx + 36} 286 {cx + 34} 318 Q{cx + 30} 340 {cx} 340 Q{cx - 30} 340 {cx - 34} 318 Z" fill="{PAW}"/>')
        if not small:
            for dx in (-14, 0, 14):
                p.append(f'<path d="M{cx + dx} 322 V336" stroke="#5c2c1c" stroke-width="4" stroke-linecap="round"/>')
    p += ["</g>", "</svg>"]
    return "\n".join(p) + "\n"


for name, small in (("pdfkub.svg", False), ("pdfkub-small.svg", True)):
    with open(f"{OUT_DIR}/{name}", "w", encoding="utf-8", newline="\n") as f:
        f.write(icon(small))
print("ok")
