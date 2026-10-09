//! Fonts embedded in a PDF for added text in any script (Thai, Vietnamese, Greek, …): a TrueType
//! face written as a Type0 font with Identity-H encoding (ISO 32000-2 §9.7), and text shaped with
//! HarfBuzz rules (harfrust) so marks such as Thai vowels and tone marks sit where the font puts
//! them. The whole face is embedded, unsubset, so the item can be edited again from the PDF alone
//! and one font object serves every item that uses the face.
//!
//! Character codes are CIDs. A glyph that stands for exactly the character it's mapped to in the
//! face's `cmap` uses its glyph ID as CID. Every other glyph gets its own CID above the glyph
//! range: the first glyph of a cluster (a Thai consonant with its vowels and tone marks, a
//! ligature) maps to the whole cluster's text in `/ToUnicode`, and the cluster's other glyphs map
//! to nothing. Text extraction then reads each cluster once, in order, whatever positions and
//! substituted glyphs shaping chose. Those extra CIDs are listed in the font's `/PCGlyphMap`.

use std::sync::Arc;

use pdfcraft_cos::{Dict, Document, ObjRef, Object, PdfString, Stream};
use skrifa::raw::{TableProvider, types::Tag};
use skrifa::{FontRef, MetadataProvider, instance::Size, string::StringId};

/// Anuphan (OFL-1.1, Cadson Demak): the bundled Thai face, shared with the interface fonts.
pub static ANUPHAN_REGULAR: &[u8] = include_bytes!("../../../assets/fonts/Anuphan-Regular.ttf");
pub static ANUPHAN_MEDIUM: &[u8] = include_bytes!("../../../assets/fonts/Anuphan-Medium.ttf");
pub static ANUPHAN_SEMIBOLD: &[u8] = include_bytes!("../../../assets/fonts/Anuphan-SemiBold.ttf");
/// Sarabun (OFL-1.1, Cadson Demak): a formal Thai face in the style of TH Sarabun New, the
/// default for added Thai text.
pub static SARABUN_REGULAR: &[u8] = include_bytes!("../../../assets/fonts/Sarabun-Regular.ttf");
pub static SARABUN_BOLD: &[u8] = include_bytes!("../../../assets/fonts/Sarabun-Bold.ttf");
pub static SARABUN_ITALIC: &[u8] = include_bytes!("../../../assets/fonts/Sarabun-Italic.ttf");
pub static SARABUN_BOLD_ITALIC: &[u8] = include_bytes!("../../../assets/fonts/Sarabun-BoldItalic.ttf");

/// Larger font files are refused: a font is untrusted input, and the whole face is embedded.
pub const MAX_FONT_BYTES: usize = 32 << 20;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EmbedError {
    #[error("the font file is larger than 32 MB")]
    TooLarge,
    #[error("not a font this app can read")]
    Unreadable,
    #[error("only TrueType fonts can be embedded so far (this one has PostScript outlines)")]
    NotTrueType,
    #[error("the font's licence doesn't allow embedding it in documents")]
    NotEmbeddable,
    #[error("the embedded font has run out of character codes")]
    TooManyCodes,
}

/// One TrueType face, ready to shape text and to embed.
#[derive(Clone, Debug)]
pub struct EmbedFace {
    /// The name shown in the font list and kept with the text (e.g. "Anuphan", "TH Sarabun New").
    pub name: String,
    /// A single-face TrueType file (a face taken out of a collection is rebuilt as one file).
    pub data: Arc<Vec<u8>>,
}

impl PartialEq for EmbedFace {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && (Arc::ptr_eq(&self.data, &other.data) || self.data == other.data)
    }
}

/// A glyph laid out by [`EmbedFace::shape`], in font units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedGlyph {
    pub gid: u16,
    /// Pen position of the glyph's origin from the start of the run, plus its own offset.
    pub x: f64,
    pub y: f64,
    /// Byte offset in the shaped text of the character(s) this glyph draws.
    pub cluster: usize,
}

/// A shaped run: glyphs in drawing order and the total advance, in font units.
#[derive(Clone, Debug, PartialEq)]
pub struct Shaped {
    pub glyphs: Vec<PlacedGlyph>,
    pub advance: f64,
    pub units_per_em: f64,
}

impl Shaped {
    /// The run's width at `size` points.
    pub fn width(&self, size: f64) -> f64 {
        self.advance * size / self.units_per_em
    }
}

impl EmbedFace {
    /// The bundled Anuphan face for Thai; `bold` picks SemiBold.
    pub fn anuphan(bold: bool) -> EmbedFace {
        let data = if bold { ANUPHAN_SEMIBOLD } else { ANUPHAN_REGULAR };
        EmbedFace { name: if bold { "Anuphan Bold".into() } else { "Anuphan".into() }, data: Arc::new(data.to_vec()) }
    }

    /// The bundled Sarabun face in the requested style.
    pub fn sarabun(bold: bool, italic: bool) -> EmbedFace {
        let (data, name) = match (bold, italic) {
            (false, false) => (SARABUN_REGULAR, "Sarabun"),
            (true, false) => (SARABUN_BOLD, "Sarabun Bold"),
            (false, true) => (SARABUN_ITALIC, "Sarabun Italic"),
            (true, true) => (SARABUN_BOLD_ITALIC, "Sarabun Bold Italic"),
        };
        EmbedFace { name: name.into(), data: Arc::new(data.to_vec()) }
    }

    /// Face `index` of a font file (`.ttf`, or `.ttc` collection), checked for TrueType outlines
    /// and for a licence that allows embedding (OS/2 `fsType`).
    pub fn from_bytes(name: &str, bytes: &[u8], index: u32) -> Result<EmbedFace, EmbedError> {
        if bytes.len() > MAX_FONT_BYTES {
            return Err(EmbedError::TooLarge);
        }
        let single = if bytes.starts_with(b"ttcf") { extract_face(bytes, index).ok_or(EmbedError::Unreadable)? } else { bytes.to_vec() };
        let font = FontRef::new(&single).map_err(|_| EmbedError::Unreadable)?;
        if font.table_data(Tag::new(b"glyf")).is_none() || font.table_data(Tag::new(b"loca")).is_none() {
            return Err(EmbedError::NotTrueType);
        }
        if !embeddable(&font) {
            return Err(EmbedError::NotEmbeddable);
        }
        Ok(EmbedFace { name: name.to_owned(), data: Arc::new(single) })
    }

    /// The parsed face (checked in `from_bytes`; the bundled faces always parse).
    fn font(&self) -> Option<FontRef<'_>> {
        FontRef::new(&self.data).ok()
    }

    /// Whether the face maps every character of `text` that is drawn (not whitespace).
    pub fn covers(&self, text: &str) -> bool {
        let Some(font) = self.font() else { return false };
        let cmap = font.charmap();
        text.chars().filter(|c| !c.is_whitespace()).all(|c| cmap.map(c).is_some())
    }

    /// Shape one line of text left to right.
    pub fn shape(&self, text: &str) -> Shaped {
        let Some(font) = self.font() else { return Shaped { glyphs: Vec::new(), advance: 0.0, units_per_em: 1000.0 } };
        let upem = f64::from(font.head().map(|h| h.units_per_em()).unwrap_or(1000).max(16));
        let data = harfrust::ShaperData::new(&font);
        let shaper = data.shaper(&font).build();
        let mut buffer = harfrust::UnicodeBuffer::new();
        buffer.push_str(text);
        buffer.guess_segment_properties();
        let out = shaper.shape(buffer, harfrust::ShapeOptions::new());
        let mut glyphs = Vec::with_capacity(out.glyph_infos().len());
        let mut pen = 0.0;
        for (info, pos) in out.glyph_infos().iter().zip(out.glyph_positions()) {
            glyphs.push(PlacedGlyph {
                gid: u16::try_from(info.glyph_id).unwrap_or(0),
                x: pen + f64::from(pos.x_offset),
                y: f64::from(pos.y_offset),
                cluster: info.cluster as usize,
            });
            pen += f64::from(pos.x_advance);
        }
        Shaped { glyphs, advance: pen, units_per_em: upem }
    }

    /// The PostScript name for `/BaseFont`: name ID 6, else the display name, without spaces.
    fn postscript_name(&self) -> String {
        let raw = self
            .font()
            .and_then(|font| font.localized_strings(StringId::POSTSCRIPT_NAME).english_or_first().map(|s| s.to_string()))
            .unwrap_or_else(|| self.name.clone());
        let clean: String = raw.chars().filter(|c| c.is_ascii_graphic() && !"()<>[]{}/%#".contains(*c)).collect();
        if clean.is_empty() { "EmbeddedFont".into() } else { clean }
    }

    /// Write the face into `doc` as a Type0 font (Identity-H, CIDs = glyph IDs) and return it.
    pub fn write(&self, doc: &mut Document) -> Result<ObjRef, EmbedError> {
        let font = self.font().ok_or(EmbedError::Unreadable)?;
        let upem = f64::from(font.head().map(|h| h.units_per_em()).unwrap_or(1000).max(16));
        let scale = 1000.0 / upem;
        let base = self.postscript_name();
        let metrics = font.metrics(Size::unscaled(), skrifa::instance::LocationRef::default());
        let glyph_metrics = font.glyph_metrics(Size::unscaled(), skrifa::instance::LocationRef::default());
        let count = u32::from(metrics.glyph_count);

        let mut file = Dict::new();
        file.set(b"Length1".to_vec(), Object::Int(self.data.len() as i64));
        let file = doc.add(Object::Stream(Stream::flate(file, &self.data)));

        let bbox = metrics.bounds.map_or([0.0, metrics.descent, upem as f32, metrics.ascent], |b| [b.x_min, b.y_min, b.x_max, b.y_max]);
        let mut fd = Dict::new();
        fd.set(b"Type".to_vec(), Object::name("FontDescriptor"));
        fd.set(b"FontName".to_vec(), Object::name(&base));
        fd.set(b"Flags".to_vec(), Object::Int(32));
        fd.set(b"FontBBox".to_vec(), Object::Array(bbox.iter().map(|v| Object::Int((f64::from(*v) * scale).round() as i64)).collect()));
        fd.set(b"ItalicAngle".to_vec(), Object::Real(f64::from(metrics.italic_angle)));
        fd.set(b"Ascent".to_vec(), Object::Int((f64::from(metrics.ascent) * scale).round() as i64));
        fd.set(b"Descent".to_vec(), Object::Int((f64::from(metrics.descent) * scale).round() as i64));
        fd.set(b"CapHeight".to_vec(), Object::Int((f64::from(metrics.cap_height.unwrap_or(metrics.ascent)) * scale).round() as i64));
        fd.set(b"StemV".to_vec(), Object::Int(80));
        fd.set(b"FontFile2".to_vec(), Object::Ref(file));
        let fd = doc.add(Object::Dict(fd));

        // Every glyph's width in one run: `[0 [w0 w1 …]]`.
        let widths: Vec<Object> = (0..count)
            .map(|g| Object::Int((f64::from(glyph_metrics.advance_width(skrifa::GlyphId::new(g)).unwrap_or(0.0)) * scale).round() as i64))
            .collect();
        let mut info = Dict::new();
        info.set(b"Registry".to_vec(), PdfString::literal(b"Adobe".to_vec()));
        info.set(b"Ordering".to_vec(), PdfString::literal(b"Identity".to_vec()));
        info.set(b"Supplement".to_vec(), Object::Int(0));
        let mut cid = Dict::new();
        cid.set(b"Type".to_vec(), Object::name("Font"));
        cid.set(b"Subtype".to_vec(), Object::name("CIDFontType2"));
        cid.set(b"BaseFont".to_vec(), Object::name(&base));
        cid.set(b"CIDSystemInfo".to_vec(), Object::Dict(info));
        cid.set(b"FontDescriptor".to_vec(), Object::Ref(fd));
        let map = doc.add(Object::Stream(Stream::flate(Dict::new(), &cid_to_gid(count, &[]))));
        cid.set(b"CIDToGIDMap".to_vec(), Object::Ref(map));
        cid.set(b"DW".to_vec(), Object::Int(1000));
        cid.set(b"W".to_vec(), Object::Array(vec![Object::Int(0), Object::Array(widths)]));
        let cid = doc.add(Object::Dict(cid));

        let to_unicode = doc.add(Object::Stream(Stream::flate(Dict::new(), &to_unicode_cmap(&font, &[]))));
        let mut t0 = Dict::new();
        t0.set(b"Type".to_vec(), Object::name("Font"));
        t0.set(b"Subtype".to_vec(), Object::name("Type0"));
        t0.set(b"BaseFont".to_vec(), Object::name(&base));
        t0.set(b"Encoding".to_vec(), Object::name("Identity-H"));
        t0.set(b"DescendantFonts".to_vec(), Object::Array(vec![Object::Ref(cid)]));
        t0.set(b"ToUnicode".to_vec(), Object::Ref(to_unicode));
        t0.set(b"PCGlyphCount".to_vec(), Object::Int(i64::from(count)));
        t0.set(b"PCGlyphMap".to_vec(), Object::Array(Vec::new()));
        Ok(doc.add(Object::Dict(t0)))
    }

    /// The character codes (2-byte CIDs) that draw `shaped`, the shaped form of `text`, with
    /// `font` (written by [`EmbedFace::write`]). New cluster CIDs are added to the font.
    pub fn codes(&self, doc: &mut Document, font: ObjRef, text: &str, shaped: &Shaped) -> Result<Vec<u16>, EmbedError> {
        let face = self.font().ok_or(EmbedError::Unreadable)?;
        let t0 = doc.get(font).as_dict().cloned().ok_or(EmbedError::Unreadable)?;
        let count = t0.get(b"PCGlyphCount").and_then(Object::as_f64).ok_or(EmbedError::Unreadable)? as u32;
        let mut extra: Vec<(u16, String)> = t0
            .get(b"PCGlyphMap")
            .map(|m| doc.resolve(m))
            .and_then(|m| m.as_array().cloned())
            .unwrap_or_default()
            .chunks(2)
            .filter_map(|e| {
                Some((u16::try_from(e[0].as_f64()? as i64).ok()?, e.get(1).and_then(|t| doc.resolve(t).as_string().map(|s| s.to_text()))?))
            })
            .collect();
        let known = extra.len();
        let plain = glyph_text(&face);
        // Cluster boundaries: each cluster runs to the next cluster's start.
        let mut starts: Vec<usize> = shaped.glyphs.iter().map(|g| g.cluster).collect();
        starts.sort_unstable();
        starts.dedup();
        let cluster_text = |c: usize| {
            let end = starts.iter().find(|&&s| s > c).copied().unwrap_or(text.len());
            text.get(c..end).unwrap_or_default().to_owned()
        };
        let mut codes = Vec::with_capacity(shaped.glyphs.len());
        for (i, g) in shaped.glyphs.iter().enumerate() {
            let alone = shaped.glyphs.iter().filter(|o| o.cluster == g.cluster).count() == 1;
            let first = shaped.glyphs[..i].iter().all(|o| o.cluster != g.cluster);
            let whole = cluster_text(g.cluster);
            if alone && plain.get(&g.gid).is_some_and(|t| *t == whole) {
                codes.push(g.gid);
                continue;
            }
            let entry = (g.gid, if first { whole } else { String::new() });
            let at = match extra.iter().position(|e| *e == entry) {
                Some(at) => at,
                None => {
                    extra.push(entry);
                    extra.len() - 1
                }
            };
            codes.push(u16::try_from(count as usize + at).map_err(|_| EmbedError::TooManyCodes)?);
        }
        if extra.len() > known {
            self.rewrite_maps(doc, font, &t0, count, &extra)?;
        }
        Ok(codes)
    }

    /// Rewrite the CID-to-glyph map, the widths and the ToUnicode map after CIDs were added.
    fn rewrite_maps(&self, doc: &mut Document, font: ObjRef, t0: &Dict, count: u32, extra: &[(u16, String)]) -> Result<(), EmbedError> {
        let face = self.font().ok_or(EmbedError::Unreadable)?;
        let upem = f64::from(face.head().map(|h| h.units_per_em()).unwrap_or(1000).max(16));
        let glyph_metrics = face.glyph_metrics(Size::unscaled(), skrifa::instance::LocationRef::default());
        let width =
            |g: u32| Object::Int((f64::from(glyph_metrics.advance_width(skrifa::GlyphId::new(g)).unwrap_or(0.0)) * 1000.0 / upem).round() as i64);
        let cid_ref = t0
            .get(b"DescendantFonts")
            .map(|d| doc.resolve(d))
            .and_then(|d| d.as_array().and_then(|a| a.first().and_then(Object::as_ref)))
            .ok_or(EmbedError::Unreadable)?;
        let mut cid = doc.get(cid_ref).as_dict().cloned().ok_or(EmbedError::Unreadable)?;
        let map_ref = cid.get(b"CIDToGIDMap").and_then(Object::as_ref).ok_or(EmbedError::Unreadable)?;
        doc.set(map_ref, Object::Stream(Stream::flate(Dict::new(), &cid_to_gid(count, extra))));
        let mut w = vec![Object::Int(0), Object::Array((0..count).map(width).collect())];
        w.push(Object::Int(i64::from(count)));
        w.push(Object::Array(extra.iter().map(|(g, _)| width(u32::from(*g))).collect()));
        cid.set(b"W".to_vec(), Object::Array(w));
        doc.set(cid_ref, Object::Dict(cid));
        let mapped: Vec<(u16, String)> = extra
            .iter()
            .enumerate()
            .filter(|(_, (_, t))| !t.is_empty())
            .filter_map(|(i, (_, t))| Some((u16::try_from(count as usize + i).ok()?, t.clone())))
            .collect();
        let tu_ref = t0.get(b"ToUnicode").and_then(Object::as_ref).ok_or(EmbedError::Unreadable)?;
        doc.set(tu_ref, Object::Stream(Stream::flate(Dict::new(), &to_unicode_cmap(&face, &mapped))));
        let mut t0 = t0.clone();
        t0.set(
            b"PCGlyphMap".to_vec(),
            Object::Array(extra.iter().flat_map(|(g, t)| [Object::Int(i64::from(*g)), Object::String(PdfString::text(t))]).collect()),
        );
        doc.set(font, Object::Dict(t0));
        Ok(())
    }

    /// The face embedded in a Type0 font that [`EmbedFace::write`] made, read back from `doc`.
    pub fn read(doc: &Document, font: ObjRef, name: &str) -> Option<EmbedFace> {
        let t0 = doc.get(font);
        let t0 = t0.as_dict()?;
        let cid = t0.get(b"DescendantFonts").map(|d| doc.resolve(d)).and_then(|d| d.as_array().and_then(|a| a.first().cloned()))?;
        let cid = doc.resolve(&cid);
        let fd = cid.as_dict()?.get(b"FontDescriptor").map(|f| doc.resolve(f))?;
        let file = fd.as_dict()?.get(b"FontFile2").map(|f| doc.resolve(f))?;
        let Object::Stream(s) = &*file else { return None };
        let bytes = s.decoded_within(MAX_FONT_BYTES).ok()?;
        EmbedFace::from_bytes(name, &bytes, 0).ok()
    }
}

/// OS/2 `fsType`: installable (0), editable (8) and preview & print (4) faces may be embedded;
/// restricted-licence (2, alone) and bitmap-only (0x200) faces may not. No OS/2 table: allowed.
pub fn embeddable(font: &FontRef<'_>) -> bool {
    let Ok(os2) = font.os2() else { return true };
    let t = os2.fs_type();
    if t & 0x0200 != 0 {
        return false;
    }
    let licence = t & 0x000f;
    !(licence & 0x0002 != 0 && licence & 0x000c == 0)
}

/// Each glyph of the face's character map with the first character that maps to it.
fn glyph_text(font: &FontRef<'_>) -> std::collections::HashMap<u16, String> {
    let mut mappings: Vec<(u32, u32)> = font.charmap().mappings().map(|(c, g)| (c, g.to_u32())).collect();
    mappings.sort_unstable();
    let mut out = std::collections::HashMap::new();
    for (c, g) in mappings {
        if let (Ok(g), Some(ch)) = (u16::try_from(g), char::from_u32(c))
            && g != 0
        {
            out.entry(g).or_insert_with(|| ch.to_string());
        }
    }
    out
}

/// The CID-to-glyph map: CIDs below `count` are glyph IDs, then the extra CIDs in order.
fn cid_to_gid(count: u32, extra: &[(u16, String)]) -> Vec<u8> {
    (0..count).filter_map(|g| u16::try_from(g).ok()).chain(extra.iter().map(|(g, _)| *g)).flat_map(u16::to_be_bytes).collect()
}

/// A ToUnicode CMap: each glyph-ID CID to its character in the face's character map, and each
/// extra cluster CID to its cluster's text.
fn to_unicode_cmap(font: &FontRef<'_>, extra: &[(u16, String)]) -> Vec<u8> {
    let mut pairs: Vec<(u16, String)> = glyph_text(font).into_iter().collect();
    pairs.extend(extra.iter().cloned());
    pairs.sort_unstable();
    let mut s = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    for chunk in pairs.chunks(100) {
        s.push_str(&format!("{} beginbfchar\n", chunk.len()));
        for (g, text) in chunk {
            let hex: String = text.encode_utf16().map(|u| format!("{u:04X}")).collect();
            s.push_str(&format!("<{g:04X}> <{hex}>\n"));
        }
        s.push_str("endbfchar\n");
    }
    s.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    s.into_bytes()
}

/// Face `index` of a TrueType collection rebuilt as a standalone font file: its table records
/// with the tables they point to, offsets renumbered (checksums kept).
fn extract_face(ttc: &[u8], index: u32) -> Option<Vec<u8>> {
    let be32 = |o: usize| ttc.get(o..o + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    let be16 = |o: usize| ttc.get(o..o + 2).map(|b| u16::from_be_bytes([b[0], b[1]]));
    let faces = be32(8)?;
    if index >= faces || faces > 4096 {
        return None;
    }
    let offset = be32(12 + 4 * index as usize)? as usize;
    let tables = usize::from(be16(offset + 4)?);
    if tables == 0 || tables > 512 {
        return None;
    }
    let header = 12 + 16 * tables;
    let mut out = Vec::with_capacity(header);
    out.extend_from_slice(ttc.get(offset..offset + 12)?);
    let mut body = Vec::new();
    for t in 0..tables {
        let rec = offset + 12 + 16 * t;
        let (tag, sum, start, len) = (be32(rec)?, be32(rec + 4)?, be32(rec + 8)? as usize, be32(rec + 12)? as usize);
        let data = ttc.get(start..start.checked_add(len)?)?;
        let at = header + body.len();
        out.extend_from_slice(&tag.to_be_bytes());
        out.extend_from_slice(&sum.to_be_bytes());
        out.extend_from_slice(&u32::try_from(at).ok()?.to_be_bytes());
        out.extend_from_slice(&u32::try_from(len).ok()?.to_be_bytes());
        body.extend_from_slice(data);
        while body.len() % 4 != 0 {
            body.push(0);
        }
    }
    out.extend_from_slice(&body);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_anuphan_is_embeddable_truetype_with_thai() {
        for bold in [false, true] {
            let face = EmbedFace::anuphan(bold);
            assert!(EmbedFace::from_bytes(&face.name, &face.data, 0).is_ok());
            assert!(face.covers("ภาษาไทย ที่นี่ Hello"));
        }
        assert!(FontRef::new(ANUPHAN_MEDIUM).is_ok());
        for (bold, italic) in [(false, false), (true, false), (false, true), (true, true)] {
            let face = EmbedFace::sarabun(bold, italic);
            assert!(EmbedFace::from_bytes(&face.name, &face.data, 0).is_ok(), "{}", face.name);
            assert!(face.covers("หนังสือราชการ ที่ ๑๒๓"), "{}", face.name);
        }
    }

    #[test]
    fn thai_marks_are_shaped_without_advancing_the_pen() {
        let face = EmbedFace::anuphan(false);
        let base = face.shape("ก");
        let marked = face.shape("กี่");
        assert_eq!(marked.glyphs.len(), 3, "{marked:?}");
        // The vowel and the tone mark sit over the consonant: the run is no wider.
        assert!((marked.advance - base.advance).abs() < 1.0, "{} vs {}", marked.advance, base.advance);
        assert!(marked.width(10.0) > 0.0);
    }

    #[test]
    fn written_font_reads_back_as_the_same_face() {
        let mut doc = Document::new_empty();
        let face = EmbedFace::anuphan(false);
        let r = face.write(&mut doc).unwrap();
        let back = EmbedFace::read(&doc, r, "Anuphan").expect("read back");
        assert_eq!(back, face);
        let t0 = doc.get(r);
        let t0 = t0.as_dict().unwrap();
        assert_eq!(t0.name(b"Subtype"), Some(&b"Type0"[..]));
        assert_eq!(t0.name(b"Encoding"), Some(&b"Identity-H"[..]));
        assert_eq!(t0.name(b"BaseFont"), Some(&b"Anuphan-Regular"[..]));
    }

    #[test]
    fn fs_type_rules() {
        let face = EmbedFace::anuphan(false);
        let mut bytes = face.data.to_vec();
        let font = FontRef::new(&bytes).unwrap();
        let at = font.table_directory().table_records().iter().find(|r| r.tag() == Tag::new(b"OS/2")).unwrap().offset() as usize + 8;
        for (fs_type, ok) in [(0u16, true), (2, false), (4, true), (8, true), (2 | 8, true), (0x200, false)] {
            bytes[at..at + 2].copy_from_slice(&fs_type.to_be_bytes());
            assert_eq!(EmbedFace::from_bytes("x", &bytes, 0).is_ok(), ok, "fsType {fs_type:#x}");
        }
    }

    #[test]
    fn faces_come_out_of_collections() {
        // A two-face collection of Anuphan Regular and SemiBold.
        let faces = [ANUPHAN_REGULAR, ANUPHAN_SEMIBOLD];
        let mut ttc = b"ttcf".to_vec();
        ttc.extend_from_slice(&[0, 1, 0, 0]);
        ttc.extend_from_slice(&2u32.to_be_bytes());
        let mut offsets = Vec::new();
        let mut body = Vec::new();
        let head = 12 + 4 * faces.len();
        for f in faces {
            // Each face's tables are rewritten to be relative to the collection start.
            let tables = usize::from(u16::from_be_bytes([f[4], f[5]]));
            let start = head + body.len();
            offsets.push(start as u32);
            let dir_len = 12 + 16 * tables;
            let data_start = start + dir_len;
            let mut dir = f[..12].to_vec();
            let mut data = Vec::new();
            for t in 0..tables {
                let rec = 12 + 16 * t;
                let off = u32::from_be_bytes(f[rec + 8..rec + 12].try_into().unwrap()) as usize;
                let len = u32::from_be_bytes(f[rec + 12..rec + 16].try_into().unwrap()) as usize;
                dir.extend_from_slice(&f[rec..rec + 8]);
                dir.extend_from_slice(&((data_start + data.len()) as u32).to_be_bytes());
                dir.extend_from_slice(&(len as u32).to_be_bytes());
                data.extend_from_slice(&f[off..off + len]);
                while data.len() % 4 != 0 {
                    data.push(0);
                }
            }
            body.extend_from_slice(&dir);
            body.extend_from_slice(&data);
        }
        for o in offsets {
            ttc.extend_from_slice(&o.to_be_bytes());
        }
        ttc.extend_from_slice(&body);
        let bold = EmbedFace::from_bytes("Anuphan SemiBold", &ttc, 1).expect("second face");
        assert_eq!(bold.postscript_name(), "Anuphan-SemiBold");
        assert!(EmbedFace::from_bytes("x", &ttc, 2).is_err());
    }
}
