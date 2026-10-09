"""Build the PdfKub app icon SVGs: the master (pdfkub.svg) and the small-size variant (pdfkub-small.svg).

The word on the speech bubble is set in Kanit Bold (SIL OFL 1.1) and converted to outlines, so the
SVG needs no font at render time.
"""
import sys
from fontTools.ttLib import TTFont
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.pens.boundsPen import BoundsPen

FONT, OUT_DIR = sys.argv[1], sys.argv[2]

INDIGO_TOP, INDIGO_BOTTOM = "#5a4ff0", "#2d2391"
PAPER, FOLD = "#fffaf2", "#e3dcf7"
LINE = "#cfc9f5"
SAFFRON_TOP, SAFFRON_BOTTOM = "#ffb02e", "#ff7417"
WHITE = "#ffffff"


def word_path(text, box):
    """Outline `text` and fit it, centred, inside box = (x0, y0, x1, y1). Returns an SVG path d."""
    font = TTFont(FONT)
    cmap, glyphs, hmtx = font.getBestCmap(), font.getGlyphSet(), font["hmtx"]
    # Lay the glyphs out by advance width. Thai above marks have no advance and are drawn over the
    # preceding consonant, so a plain left-to-right walk is enough for this word.
    placed, x = [], 0
    for ch in text:
        name = cmap[ord(ch)]
        placed.append((name, x))
        x += hmtx[name][0]
    bounds = BoundsPen(glyphs)
    for name, dx in placed:
        glyphs[name].draw(TransformPen(bounds, (1, 0, 0, 1, dx, 0)))
    gx0, gy0, gx1, gy1 = bounds.bounds
    bx0, by0, bx1, by1 = box
    scale = min((bx1 - bx0) / (gx1 - gx0), (by1 - by0) / (gy1 - gy0))
    ox = bx0 + ((bx1 - bx0) - (gx1 - gx0) * scale) / 2 - gx0 * scale
    oy = by0 + ((by1 - by0) - (gy1 - gy0) * scale) / 2 + gy1 * scale  # font y points up
    pen = SVGPathPen(glyphs, ntos=lambda v: f"{v:.1f}".rstrip("0").rstrip("."))
    for name, dx in placed:
        glyphs[name].draw(TransformPen(pen, (scale, 0, 0, -scale, ox + dx * scale, oy)))
    return pen.getCommands()


def icon(small):
    page = "M150 84 H300 L366 150 V372 Q366 392 346 392 H150 Q130 392 130 372 V104 Q130 84 150 84 Z"
    fold = "M300 84 V132 Q300 150 318 150 H366 Z"
    bubble = (
        "M214 262 H418 Q448 262 448 292 V378 Q448 408 418 408 H262 L214 446 L222 408 H214"
        " Q184 408 184 378 V292 Q184 262 214 262 Z"
    )
    if small:
        lines = [(164, 196, 268), (164, 236, 236)]
        line_w = 22
    else:
        lines = [(164, 182, 300), (164, 214, 272), (164, 246, 228)]
        line_w = 14
    parts = [
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">',
        "<defs>",
        f'<linearGradient id="bg" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="{INDIGO_TOP}"/>'
        f'<stop offset="1" stop-color="{INDIGO_BOTTOM}"/></linearGradient>',
        f'<linearGradient id="bubble" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{SAFFRON_TOP}"/>'
        f'<stop offset="1" stop-color="{SAFFRON_BOTTOM}"/></linearGradient>',
        '<clipPath id="tile"><rect width="512" height="512" rx="112"/></clipPath>',
        "</defs>",
        '<g clip-path="url(#tile)">',
        '<rect width="512" height="512" fill="url(#bg)"/>',
        # soft shadow under the page and the bubble
        f'<path d="{page}" transform="translate(8 14)" fill="#1b1260" opacity="0.35"/>',
        f'<path d="{page}" fill="{PAPER}"/>',
        f'<path d="{fold}" fill="{FOLD}"/>',
    ]
    for x0, y, x1 in lines:
        parts.append(
            f'<path d="M{x0} {y} H{x1}" stroke="{LINE}" stroke-width="{line_w}" stroke-linecap="round"/>'
        )
    parts += [
        f'<path d="{bubble}" transform="translate(6 12)" fill="#1b1260" opacity="0.35"/>',
        f'<path d="{bubble}" fill="url(#bubble)"/>',
    ]
    if not small:
        parts.append(f'<path d="{word_path("ครับ", (214, 284, 418, 384))}" fill="{WHITE}"/>')
    else:
        for cx in (256, 316, 376):
            parts.append(f'<circle cx="{cx}" cy="335" r="20" fill="{WHITE}"/>')
    parts += ["</g>", "</svg>"]
    return "\n".join(parts) + "\n"


for name, small in (("pdfkub.svg", False), ("pdfkub-small.svg", True)):
    with open(f"{OUT_DIR}/{name}", "w", encoding="utf-8", newline="\n") as f:
        f.write(icon(small))
print("ok")
