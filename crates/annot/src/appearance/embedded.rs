//! PdfKub: appearances whose text needs more than WinAnsi (Thai), drawn with Sarabun embedded in
//! the document. Each draws what [`build`] draws with the text left out, then the text on top, in
//! the same form space (`/BBox`, and the `/Matrix` of a box or stamp on a turned page).

use pdfcraft_cos::{Dict, Object, PdfString, Stream};
use pdfcraft_fonts::EmbedFace;

use super::{border_width, build, color, form, nums, parse_da, rg};
use crate::n;

/// The normal appearance of a FreeText annotation (text box, typewriter, callout) or a PdfKub
/// stamp's "By … at …" line whose text needs more than WinAnsi (Thai), with that text in Sarabun
/// embedded in `doc`. `None` for anything else, where [`build`] applies.
pub fn build_embedded(doc: &mut pdfcraft_cos::Document, d: &Dict) -> Option<Stream> {
    if d.name(b"Subtype") == Some(b"Stamp") {
        return stamp_embedded(doc, d);
    }
    if d.name(b"Subtype") != Some(b"FreeText") {
        return None;
    }
    let text = d.get(b"Contents").and_then(|o| o.as_string()).map(PdfString::to_text).unwrap_or_default();
    // Only text Sarabun has every letter of: anything else (Chinese, …) is left to `build`, which
    // draws `?` and marks the comment, so it is never silently lost.
    if pdfcraft_fonts::win_ansi_covers(&text) || !EmbedFace::sarabun(false, false).covers(&text) {
        return None;
    }
    // The box, border and callout as `build` draws them (its text clip stays set), then the text.
    let mut blank = d.clone();
    blank.set(b"Contents".to_vec(), PdfString::text(""));
    let base = build(&blank)?;
    let mut out = base.decoded().ok()?;
    let mut res = base.dict.get(b"Resources").and_then(|r| r.as_dict().cloned()).unwrap_or_default();
    let full = bbox(&base)?;
    let mut rect = full;
    if d.contains(b"CL") {
        let rd = nums(d, b"RD").filter(|r| r.len() == 4 && r.iter().all(|x| *x >= 0.0))?;
        rect = [full[0] + rd[0], full[1] + rd[1], full[2] - rd[2], full[3] - rd[3]];
    }
    let (text_color, size) = parse_da(d);
    let bw = if d.contains(b"BS") || d.contains(b"Border") { border_width(d) } else { 0.0 };
    let pad = 2.0 + bw;
    let width = (rect[2] - rect[0] - 2.0 * pad).max(1.0);
    let q = d.int(b"Q").unwrap_or(0);
    let face = EmbedFace::sarabun(false, false);
    let font = pdfcraft_fonts::embedded_font(doc, &face).ok()?;
    let name = format!("PCE{}", font.num);
    let (first, step) = face.line_metrics();
    out.extend(format!("BT\n/{name} {} Tf\n{}", n(size), rg(text_color)).bytes());
    let mut y = rect[3] - pad - size * first;
    for line in pdfcraft_fonts::wrap_fitting(&text, |s| face.shape(s).width(size) <= width) {
        if y < rect[1] - size {
            break;
        }
        let shaped = face.shape(&line);
        let w = shaped.width(size);
        let scale = size / shaped.units_per_em;
        let x0 = match q {
            1 => rect[0] + pad + (width - w) / 2.0,
            2 => rect[2] - pad - w,
            _ => rect[0] + pad,
        };
        let codes = face.codes(doc, font, &line, &shaped).ok()?;
        let units: String = line.encode_utf16().map(|u| format!("{u:04X}")).collect();
        out.extend(format!("/Span << /ActualText <FEFF{units}> >> BDC\n").bytes());
        for (g, code) in shaped.glyphs.iter().zip(codes) {
            out.extend(format!("1 0 0 1 {} {} Tm <{code:04X}> Tj\n", n(x0 + g.x * scale), n(y + g.y * scale)).bytes());
        }
        out.extend_from_slice(b"EMC\n");
        y -= size * step;
    }
    out.extend_from_slice(b"ET\n");
    Some(finish(&base, full, &out, res_with(&mut res, name, font)))
}

/// A PdfKub stamp whose "By … at …" line needs more than WinAnsi (a Thai name): the stamp as
/// [`build`] draws it, with that line in Sarabun embedded in `doc`.
fn stamp_embedded(doc: &mut pdfcraft_cos::Document, d: &Dict) -> Option<Stream> {
    let by = d.get(b"PCByLine").and_then(|o| o.as_string()).map(PdfString::to_text)?;
    if pdfcraft_fonts::win_ansi_covers(&by) || !matches!(d.get(b"PCStamp"), Some(Object::Bool(true))) || !EmbedFace::sarabun(false, false).covers(&by)
    {
        return None;
    }
    let kind = crate::StampKind::from_name(d.name(b"Name")?)?;
    // The same layout with an invisible by-line, then the real one on top.
    let mut blank = d.clone();
    blank.set(b"PCByLine".to_vec(), PdfString::text(" "));
    let base = build(&blank)?;
    let mut out = base.decoded().ok()?;
    let mut res = base.dict.get(b"Resources").and_then(|r| r.as_dict().cloned()).unwrap_or_default();
    let rect = bbox(&base)?;
    let [x0, y0, x1, y1] = rect;
    let h = y1 - y0;
    let col = color(d, b"C").flatten().unwrap_or(kind.color());
    let sign_here = kind.group() == crate::StampGroup::SignHere;
    let text_col = if sign_here { [1.0, 1.0, 1.0] } else { col };
    // The text's left and right ends inside the frame, as `symbols::stamp` lays them out.
    let lw = (h * 0.07).clamp(1.0, 3.0);
    let area = if sign_here { (x0 + h * 0.45, x1 - lw * 2.0) } else { (x0 + lw * 2.0, x1 - lw * 2.0) };
    let face = EmbedFace::sarabun(false, false);
    let font = pdfcraft_fonts::embedded_font(doc, &face).ok()?;
    let shaped = face.shape(&by);
    let mut size = h * 0.22;
    if shaped.width(size) > area.1 - area.0 - 4.0 {
        size *= (area.1 - area.0 - 4.0) / shaped.width(size);
    }
    let scale = size / shaped.units_per_em;
    let x = area.0 + (area.1 - area.0 - shaped.width(size)) / 2.0;
    let y = y0 + h * 0.18;
    let codes = face.codes(doc, font, &by, &shaped).ok()?;
    let name = format!("PCE{}", font.num);
    let units: String = by.encode_utf16().map(|u| format!("{u:04X}")).collect();
    out.extend(format!("BT\n{}/{name} {} Tf\n/Span << /ActualText <FEFF{units}> >> BDC\n", rg(text_col), n(size)).bytes());
    for (g, code) in shaped.glyphs.iter().zip(codes) {
        out.extend(format!("1 0 0 1 {} {} Tm <{code:04X}> Tj\n", n(x + g.x * scale), n(y + g.y * scale)).bytes());
    }
    out.extend_from_slice(b"EMC\nET\n");
    Some(finish(&base, rect, &out, res_with(&mut res, name, font)))
}

/// The drawing box of `build`'s appearance, normalised.
fn bbox(base: &Stream) -> Option<[f64; 4]> {
    let b = nums(&base.dict, b"BBox").filter(|r| r.len() == 4 && r.iter().all(|x| x.is_finite()))?;
    Some([b[0].min(b[2]), b[1].min(b[3]), b[0].max(b[2]), b[1].max(b[3])])
}

/// `res` with the embedded font added under `name`.
fn res_with(res: &mut Dict, name: String, font: pdfcraft_cos::ObjRef) -> Dict {
    let mut fonts = res.get(b"Font").and_then(|f| f.as_dict().cloned()).unwrap_or_default();
    fonts.set(name.into_bytes(), Object::Ref(font));
    res.set(b"Font".to_vec(), Object::Dict(fonts));
    std::mem::take(res)
}

/// The new form, keeping the `/Matrix` that maps a turned page's upright box into `/Rect`.
fn finish(base: &Stream, bbox: [f64; 4], content: &[u8], res: Dict) -> Stream {
    let mut s = form(bbox, content, res);
    if let Some(m) = base.dict.get(b"Matrix") {
        s.dict.set(b"Matrix".to_vec(), m.clone());
    }
    s
}
