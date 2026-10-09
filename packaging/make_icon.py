"""Build the PdfKub app icon SVGs: the master (pdfkub.svg) and the small-size variant (pdfkub-small.svg).

A flat red panda rests its paws on a white card that shows the app's symbol (a PDF page), on a
blue frosted-glass tile: a diagonal gradient with two blurred lights behind it, a light
rim and a diagonal sheen. The same panda as the other *Kub apps; each app has
its own tile colour and symbol. Everything is plain SVG shapes, so no font or external artwork is
needed. The small variant drops the shadow and fine detail for 24 px and below.

  python3 packaging/make_icon.py assets/app-icon
"""
import sys

OUT_DIR = sys.argv[1]
STEM = "pdfkub"
SYMBOL = "page"

TILE_LIGHT, TILE_DARK = "#4c8dff", "#1d48c9"  # top-left -> bottom-right
GLOW_A, GLOW_B = "#7fe3ff", "#8a5cff"  # the soft lights behind the glass
FUR, FUR_DEEP = "#f2732f", "#d85a1e"  # head and ears; paws
CREAM = "#fff5e8"  # inner ears, brows, cheeks and muzzle
MARK = "#a3391a"  # tear marks
INK = "#22140f"  # eyes, nose, mouth
CARD = "#ffffff"
SUN = "#ffc53d"


def face(small):
    """The red panda's head, centred on (256, 190)."""
    p = ['<g transform="translate(256 190) scale(1.05)">']
    for sx in (1, -1):
        t = "" if sx == 1 else ' transform="scale(-1 1)"'
        p.append(
            f'<g{t}><path d="M-104 -22 C-112 -88 -92 -118 -58 -114 C-36 -110 -26 -84 -28 -58 Z" fill="{FUR}"/>'
            f'<path d="M-92 -34 C-98 -80 -86 -100 -62 -98 C-46 -94 -40 -78 -42 -62 Z" fill="{CREAM}"/></g>'
        )
    p.append(f'<ellipse cx="0" cy="0" rx="104" ry="86" fill="{FUR}"/>')
    p.append(
        '<path d="M-104 6 C-104 58 -54 86 0 86 C54 86 104 58 104 6 C84 -8 52 2 30 18 C14 10 -14 10 -30 18 '
        f'C-52 2 -84 -8 -104 6 Z" fill="{CREAM}"/>'
    )
    p.append(f'<ellipse cx="-40" cy="-40" rx="17" ry="10" fill="{CREAM}"/><ellipse cx="40" cy="-40" rx="17" ry="10" fill="{CREAM}"/>')
    for sx in (1, -1):
        p.append(
            f'<path transform="scale({sx} 1)" d="M-50 -18 C-34 -20 -26 -4 -28 16 C-30 34 -36 52 -42 62 '
            f'C-52 44 -58 22 -58 4 C-58 -8 -56 -16 -50 -18 Z" fill="{MARK}"/>'
        )
    eye = 15 if small else 13
    p.append(f'<circle cx="-42" cy="0" r="{eye}" fill="{INK}"/><circle cx="42" cy="0" r="{eye}" fill="{INK}"/>')
    if not small:
        p.append('<circle cx="-37" cy="-5" r="4.5" fill="#fff"/><circle cx="47" cy="-5" r="4.5" fill="#fff"/>')
    p.append(f'<path d="M-15 30 Q0 24 15 30 Q14 44 0 48 Q-14 44 -15 30 Z" fill="{INK}"/>')
    if not small:
        p.append(f'<path d="M0 48 V56 M-12 60 Q0 68 12 60" stroke="{INK}" stroke-width="4.5" fill="none" stroke-linecap="round"/>')
    p.append("</g>")
    return "".join(p)


def symbol(small):
    """The app's symbol on the card, in the tile's dark colour."""
    c = TILE_DARK
    if SYMBOL == "page":
        # a page: folded corner and three text bars
        bars = 2 if small else 3
        out = [f'<path d="M318 322 V352 Q318 360 326 360 H356 Z" fill="{c}" opacity="0.3"/>'] if not small else []
        for i in range(bars):
            w = (150 if i < 2 else 96) if not small else 150
            h = 16 if not small else 26
            y = (384 + i * 34) if not small else (382 + i * 50)
            out.append(f'<rect x="181" y="{y}" width="{w}" height="{h}" rx="{h / 2}" fill="{c}"/>')
        return "".join(out)
    if SYMBOL == "photo":
        # a photo: sun and two mountains in a rounded frame
        sw = 10 if not small else 16
        return (
            f'<rect x="170" y="352" width="172" height="128" rx="20" fill="{c}" opacity="0.14"/>'
            f'<rect x="170" y="352" width="172" height="128" rx="20" fill="none" stroke="{c}" stroke-width="{sw}"/>'
            f'<circle cx="302" cy="386" r="{15 if not small else 20}" fill="{SUN}"/>'
            f'<path d="M176 474 L232 404 L268 446 L288 424 L336 474 Z" fill="{c}"/>'
        )
    # blueprint: a bolt circle with centre lines and a dimension line
    if small:
        return (
            f'<circle cx="256" cy="414" r="56" fill="none" stroke="{c}" stroke-width="16"/>'
            f'<path d="M256 340 V488 M182 414 H330" stroke="{c}" stroke-width="10" stroke-linecap="round"/>'
        )
    return (
        f'<circle cx="256" cy="404" r="50" fill="none" stroke="{c}" stroke-width="10"/>'
        f'<circle cx="256" cy="404" r="15" fill="none" stroke="{c}" stroke-width="8"/>'
        f'<path d="M184 404 H328 M256 332 V476" stroke="{c}" stroke-width="4" stroke-dasharray="14 6"/>'
        f'<path d="M206 488 H306" stroke="{c}" stroke-width="5"/>'
        f'<path d="M206 488 l14 -8 v16 Z M306 488 l-14 -8 v16 Z" fill="{c}"/>'
    )


def icon(small):
    p = [
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">',
        "<defs>",
        f'<linearGradient id="bg" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="{TILE_LIGHT}"/>'
        f'<stop offset="1" stop-color="{TILE_DARK}"/></linearGradient>',
        '<linearGradient id="gloss" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#ffffff" stop-opacity="0.22"/>'
        '<stop offset="0.5" stop-color="#ffffff" stop-opacity="0"/></linearGradient>',
        '<clipPath id="tile"><rect width="512" height="512" rx="114"/></clipPath>',
        '<filter id="shadow" x="-20%" y="-20%" width="140%" height="140%">'
        '<feDropShadow dx="0" dy="10" stdDeviation="12" flood-color="#000000" flood-opacity="0.28"/></filter>',
        '<filter id="blur" x="-50%" y="-50%" width="200%" height="200%"><feGaussianBlur stdDeviation="46"/></filter>',
        '<linearGradient id="sheen" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#ffffff" stop-opacity="0.34"/>'
        '<stop offset="0.45" stop-color="#ffffff" stop-opacity="0.06"/><stop offset="1" stop-color="#ffffff" stop-opacity="0"/></linearGradient>',
        '<linearGradient id="rim" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#ffffff" stop-opacity="0.85"/>'
        '<stop offset="0.5" stop-color="#ffffff" stop-opacity="0.15"/><stop offset="1" stop-color="#ffffff" stop-opacity="0.5"/></linearGradient>',
        "</defs>",
        '<g clip-path="url(#tile)">',
        '<rect width="512" height="512" fill="url(#bg)"/>',
    ]
    # Glass: coloured lights glow through a frosted pane, which catches a diagonal sheen.
    if small:
        p.append('<rect width="512" height="512" fill="url(#gloss)"/>')
    else:
        p += [
            f'<g filter="url(#blur)"><circle cx="96" cy="120" r="150" fill="{GLOW_A}" opacity="0.85"/>'
            f'<circle cx="440" cy="430" r="170" fill="{GLOW_B}" opacity="0.75"/></g>',
            '<rect width="512" height="512" fill="#ffffff" opacity="0.07"/>',
            '<path d="M0 0 H512 V150 C380 230 170 120 0 250 Z" fill="url(#sheen)"/>',
        ]
    # The card runs off the bottom of the tile.
    card = f'<rect x="126" y="268" width="260" height="300" rx="36" fill="{CARD}"/>'
    p.append(card if small else f'<g filter="url(#shadow)">{card}</g>')
    p.append(symbol(small))
    p.append(face(small))
    for x in (196, 316):
        p.append(f'<ellipse cx="{x}" cy="290" rx="34" ry="24" fill="{FUR_DEEP}"/>')
    # The pane's lit rim, drawn last so it runs over everything at the tile edge.
    rim_w = 10 if small else 6
    p.append(f'<rect x="{rim_w / 2}" y="{rim_w / 2}" width="{512 - rim_w}" height="{512 - rim_w}" rx="{114 - rim_w / 2}" '
             f'fill="none" stroke="url(#rim)" stroke-width="{rim_w}"/>')
    p += ["</g>", "</svg>"]
    return "\n".join(p) + "\n"


for name, small in ((f"{STEM}.svg", False), (f"{STEM}-small.svg", True)):
    with open(f"{OUT_DIR}/{name}", "w", encoding="utf-8", newline="\n") as f:
        f.write(icon(small))
print("ok")
