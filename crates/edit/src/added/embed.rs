//! PdfKub: added text drawn with an embedded TrueType face (`pdfcraft_fonts::EmbedFace`): Thai in
//! the bundled Sarabun, or a face picked from the font list (any script it has). The face is
//! written once per document as a Type0 font and shared by every item that uses it; the item's
//! `/PCAdded` names it (`/EmbedFont`, `/EmbedFontObj`) so it is read back and redrawn alike.

use super::layout::{is_arabic, is_hebrew};
use super::types::{AddedText, Align, TAG};
use super::*;

impl AddedText {
    /// The face the text is drawn with, if any: the chosen one, else Sarabun (a formal Thai face)
    /// for text the standard fonts can't show. Arabic and Hebrew without a chosen face are drawn from
    /// the craft-fonts faces instead (Type 3 glyphs in display order, see `draw_shaped`).
    pub fn face(&self) -> Option<EmbedFace> {
        match &self.font {
            Some(f) => Some(f.clone()),
            None if self.text.chars().any(|c| is_arabic(c) || is_hebrew(c)) => None,
            None if !win_ansi_covers(&self.text) => Some(EmbedFace::sarabun(self.bold, self.italic)),
            None => None,
        }
    }

    /// The first baseline's distance below the box top and the distance between baselines, in
    /// ems: the standard fonts' 0.95 and 1.2, or the embedded face's own (Thai needs more room
    /// for stacked vowels and tone marks).
    pub(super) fn spacing(&self, face: Option<&EmbedFace>) -> (f64, f64) {
        face.map_or((0.95, 1.2), EmbedFace::line_metrics)
    }

    /// The advance of `s` in `face` at this item's size, with the character spacing after each
    /// character (as `latin_width` counts it).
    fn face_width(&self, face: &EmbedFace, s: &str) -> f64 {
        face.shape(s).width(self.size) + self.char_spacing * s.chars().count() as f64
    }
}

/// The embedded Type0 font of a text item and each line's character codes.
pub(super) struct EmbeddedText {
    font: ObjRef,
    codes: Vec<Vec<u16>>,
}

/// [`super::layout::wrapped`] for text drawn with `face`: wrapped by the face's shaped widths,
/// left to right.
pub(super) fn wrapped(t: &AddedText, face: &EmbedFace, width: f64) -> Vec<(String, bool)> {
    pdfcraft_fonts::wrap_fitting(&t.text, |s| t.face_width(face, s) <= width).into_iter().map(|l| (l, false)).collect()
}

/// [`super::layout::first_undrawable`] for text drawn with `face`: a character it has no glyph for.
pub(super) fn first_undrawable(t: &AddedText, face: &EmbedFace) -> Option<char> {
    // Lines split on LF only: any other control character would be drawn as a missing glyph.
    let missing = |c: char| !c.is_whitespace() && !face.covers(c.encode_utf8(&mut [0; 4]));
    t.text.split('\n').flat_map(str::chars).find(|&c| c.is_control() || missing(c))
}

/// The check `validate` makes for text drawn with an embedded face; `None` when the standard checks
/// apply.
pub(super) fn validate(t: &AddedText) -> Option<Result<(), EditError>> {
    let face = t.face()?;
    Some(match first_undrawable(t, &face) {
        Some(c) => Err(EditError::Invalid(format!("the font {} has no letters for \"{c}\" (U+{:04X})", face.name, u32::from(c)))),
        None => Ok(()),
    })
}

/// Embed the face of a text item that has one (or find the one another item embedded), with the
/// codes of each line's glyphs.
pub(super) fn embed(doc: &mut Document, c: &Content) -> Result<Option<EmbeddedText>, EditError> {
    let Content::Text(t) = c else { return Ok(None) };
    let Some(face) = t.face() else { return Ok(None) };
    let font = match shared_font(doc, &face) {
        Some(r) => r,
        None => pdfcraft_fonts::embedded_font(doc, &face).map_err(|e| EditError::Invalid(e.to_string()))?,
    };
    let lines = lines(t);
    let shaped: Vec<_> = lines.iter().map(|l| face.shape(l)).collect();
    let runs: Vec<_> = lines.iter().map(String::as_str).zip(&shaped).collect();
    let codes = face.codes_for(doc, font, &runs).map_err(|e| EditError::Invalid(e.to_string()))?;
    Ok(Some(EmbeddedText { font, codes }))
}

/// Draw `t` with its embedded face into `out` (inside the item's `q … cm`), naming the font in `res`.
pub(super) fn draw(t: &AddedText, embedded: Option<&EmbeddedText>, out: &mut Vec<u8>, res: &mut Dict) -> Result<(), EditError> {
    let face = t.face().ok_or_else(|| EditError::Invalid("no font".into()))?;
    let embedded = embedded.ok_or_else(|| EditError::Invalid("the font wasn't embedded".into()))?;
    let font = embedded.font;
    let name = format!("PCE{}", font.num);
    let mut fonts = Dict::new();
    fonts.set(name.clone().into_bytes(), Object::Ref(font));
    res.set(b"Font".to_vec(), Object::Dict(fonts));
    let r = text_rect(t);
    let [cr, cg, cb] = t.color.map(|v| v.clamp(0.0, 1.0));
    out.extend(format!("BT /{name} {} Tf {} {} {} rg\n", n(t.size), n(cr), n(cg), n(cb)).bytes());
    let all = lines(t);
    let (first, step) = t.spacing(Some(&face));
    for (i, line) in all.iter().enumerate() {
        let shaped = face.shape(line);
        let scale = t.size / shaped.units_per_em;
        let w = t.face_width(&face, line);
        let x0 = match t.align {
            Align::Left | Align::Justify => r[0],
            Align::Center => r[0] + ((r[2] - r[0]) - w) / 2.0,
            Align::Right => r[2] - w,
        };
        let y0 = r[3] - (i as f64 * step + first) * t.size;
        let spaces = line.matches(' ').count();
        let extra = if t.align == Align::Justify && i + 1 < all.len() && spaces > 0 { ((r[2] - r[0]) - w).max(0.0) / spaces as f64 } else { 0.0 };
        // The exact text for search, copy and screen readers, whatever glyphs shaping chose.
        let units: String = line.encode_utf16().map(|u| format!("{u:04X}")).collect();
        out.extend(format!("/Span << /ActualText <FEFF{units}> >> BDC\n").bytes());
        // Each cluster moves by the justification after every space and the character spacing
        // after every character before it.
        let mut shift = 0.0;
        let mut last_cluster = usize::MAX;
        // Characters before the current cluster (bytes, count), counted on from the last cluster.
        let (mut at, mut before) = (0, 0usize);
        let codes = embedded.codes.get(i).ok_or_else(|| EditError::Invalid("the text changed while it was drawn".into()))?;
        for (g, code) in shaped.glyphs.iter().zip(codes) {
            if g.cluster != last_cluster {
                if last_cluster != usize::MAX && line.get(..g.cluster).is_some_and(|s| s.ends_with(' ')) {
                    shift += extra;
                }
                before = match line.get(at..g.cluster) {
                    Some(s) if g.cluster >= at => before + s.chars().count(),
                    _ => line.get(..g.cluster).map_or(0, |s| s.chars().count()),
                };
                at = g.cluster;
                last_cluster = g.cluster;
            }
            let x = x0 + g.x * scale + shift + t.char_spacing * before as f64;
            let y = y0 + g.y * scale;
            out.extend(format!("1 0 0 1 {} {} Tm <{code:04X}> Tj\n", n(x), n(y)).bytes());
        }
        out.extend_from_slice(b"EMC\n");
    }
    out.extend_from_slice(b"ET\n");
    Ok(())
}

/// A chosen embedded face in the item's parameters: its name and the file's length (to share one
/// font object between items with the same face).
pub(super) fn params(t: &AddedText, d: &mut Dict) {
    if let Some(f) = &t.font {
        d.set(b"EmbedFont".to_vec(), PdfString::text(&f.name));
        d.set(b"EmbedLength".to_vec(), Object::Int(f.data.len() as i64));
    }
}

/// The font object and face name of an item drawn with an embedded face, stored with the item.
pub(super) fn mark(pd: &mut Dict, embedded: Option<&EmbeddedText>, c: &Content) {
    if let (Some(e), Content::Text(t)) = (embedded, c) {
        pd.set(b"EmbedFontObj".to_vec(), Object::Ref(e.font));
        let face = t.face().map(|f| f.name).unwrap_or_default();
        pd.set(b"EmbedFace".to_vec(), PdfString::text(&face));
    }
}

/// The chosen face of a saved text item, read back from the font it embedded.
pub(super) fn read_font(doc: &Document, d: &Dict) -> Option<EmbedFace> {
    let name = d.get(b"EmbedFont").and_then(|n| doc.resolve(n).as_string().map(|s| s.to_text()))?;
    EmbedFace::read(doc, font_obj(d)?, &name)
}

/// The embedded font object of a saved text item, if it has one.
fn font_obj(d: &Dict) -> Option<ObjRef> {
    d.get(b"EmbedFontObj").and_then(Object::as_ref)
}

/// The font object another added item already embedded for `face`, if any.
fn shared_font(doc: &Document, face: &EmbedFace) -> Option<ObjRef> {
    for p in page_list(doc) {
        let list: Vec<Object> = match p.dict.get(b"Contents") {
            Some(c) => match &*doc.resolve(c) {
                Object::Array(a) => a.clone(),
                _ => vec![c.clone()],
            },
            None => Vec::new(),
        };
        for o in list {
            let Some(r) = o.as_ref() else { continue };
            let obj = doc.get(r);
            let Some(d) = obj.as_dict() else { continue };
            if d.name(b"PCMark") != Some(TAG.as_bytes()) {
                continue;
            }
            let Some(pd) = d.get(b"PCAdded").map(|p| doc.resolve(p)).and_then(|p| p.as_dict().cloned()) else { continue };
            let same = pd.get(b"EmbedFace").and_then(|n| doc.resolve(n).as_string().map(|s| s.to_text())).is_some_and(|n| n == face.name);
            let Some(font) = font_obj(&pd) else { continue };
            if same && EmbedFace::read(doc, font, &face.name).is_some_and(|f| f.data == face.data) {
                return Some(font);
            }
        }
    }
    None
}
