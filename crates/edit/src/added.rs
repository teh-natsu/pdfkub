//! Edit a PDF ▸ Add content: text and images added as page content (execution plan M7.4).
//!
//! Each added item is its own content stream on the page, tagged `/PCMark /Added`, with its
//! parameters (text, font, size, colour, alignment, box; or image and box) kept in the stream
//! dictionary under `/PCAdded`. That keeps the item editable later (move, resize, retype,
//! reformat, delete) without content-stream surgery, while every viewer sees ordinary page
//! content. Boxes are in display space (points, origin at the bottom-left of the page as shown,
//! after `/UserUnit` and `/Rotate`), so items stay upright on rotated pages; an item records the
//! `/UserUnit` it was written for.

use std::collections::{HashMap, HashSet};

use pdfcraft_cos::{Dict, Document, ObjRef, Object, PdfString, Stream};
use pdfcraft_fonts::{
    EmbedFace, GlyphError, GlyphOutline, ShapedCluster, ShapedScript, Std14, literal, shape_arabic, shape_hebrew, shape_thai, shaped_glyph, win_ansi,
    win_ansi_covers,
};
use unicode_bidi::{Level, ParagraphBidiInfo};

use crate::{EditError, MarkKind, check, contents, matrix_number, n, page_list, place_tagged};

const TAG: &str = "Added";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Family {
    #[default]
    Helvetica,
    Times,
    Courier,
}

impl Family {
    pub fn label(self) -> &'static str {
        match self {
            Family::Helvetica => "Helvetica",
            Family::Times => "Times",
            Family::Courier => "Courier",
        }
    }

    /// The standard-14 font name for this family and style.
    pub fn base_font(self, bold: bool, italic: bool) -> &'static str {
        match (self, bold, italic) {
            (Family::Helvetica, false, false) => "Helvetica",
            (Family::Helvetica, true, false) => "Helvetica-Bold",
            (Family::Helvetica, false, true) => "Helvetica-Oblique",
            (Family::Helvetica, true, true) => "Helvetica-BoldOblique",
            (Family::Times, false, false) => "Times-Roman",
            (Family::Times, true, false) => "Times-Bold",
            (Family::Times, false, true) => "Times-Italic",
            (Family::Times, true, true) => "Times-BoldItalic",
            (Family::Courier, false, false) => "Courier",
            (Family::Courier, true, false) => "Courier-Bold",
            (Family::Courier, false, true) => "Courier-Oblique",
            (Family::Courier, true, true) => "Courier-BoldOblique",
        }
    }

    fn from_base(base: &[u8]) -> (Family, bool, bool) {
        let s = String::from_utf8_lossy(base);
        let family = if s.starts_with("Times") {
            Family::Times
        } else if s.starts_with("Courier") {
            Family::Courier
        } else {
            Family::Helvetica
        };
        (family, s.contains("Bold"), s.contains("Italic") || s.contains("Oblique"))
    }

    /// Advance width of `s` at `size` points, from the published standard-14 metrics.
    ///
    /// `italic` matters: the item is written out as the oblique/italic face (see [`Family::base_font`]),
    /// whose published widths differ from the upright face (Times-Italic `A` is 611, Times-Roman 722).
    pub fn width(self, s: &str, size: f64, bold: bool, italic: bool) -> f64 {
        self.std14(bold, italic).text_width(s, size)
    }

    /// The standard-14 face this family means at the given style.
    pub fn std14(self, bold: bool, italic: bool) -> Std14 {
        match (self, bold, italic) {
            (Family::Courier, false, false) => Std14::Courier,
            (Family::Courier, true, false) => Std14::CourierBold,
            (Family::Courier, false, true) => Std14::CourierOblique,
            (Family::Courier, true, true) => Std14::CourierBoldOblique,
            (Family::Times, false, false) => Std14::TimesRoman,
            (Family::Times, true, false) => Std14::TimesBold,
            (Family::Times, false, true) => Std14::TimesItalic,
            (Family::Times, true, true) => Std14::TimesBoldItalic,
            (Family::Helvetica, false, false) => Std14::Helvetica,
            (Family::Helvetica, true, false) => Std14::HelveticaBold,
            (Family::Helvetica, false, true) => Std14::HelveticaOblique,
            (Family::Helvetica, true, true) => Std14::HelveticaBoldOblique,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
    /// Lines (but the last) stretched to the box's width.
    Justify,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AddedText {
    /// The text box in display space; its width wraps the text and its top is the first line's
    /// top. The height follows the text.
    pub rect: [f64; 4],
    pub text: String,
    pub family: Family,
    pub bold: bool,
    pub italic: bool,
    pub size: f64,
    pub color: [f64; 3],
    pub align: Align,
    /// An embedded TrueType face (any script); `None` draws with the standard font `family`,
    /// or with the bundled Sarabun when the text needs characters outside WinAnsi (Thai).
    pub font: Option<EmbedFace>,
}

impl AddedText {
    /// The face the text is drawn with, if any: the chosen one, else Sarabun (a formal Thai face)
    /// for text the standard fonts can't show. Arabic and Hebrew without a chosen face are drawn from
    /// the craft-fonts faces instead (Type 3 glyphs in display order, see `draw_arabic`).
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
    fn spacing(&self, face: Option<&EmbedFace>) -> (f64, f64) {
        face.map_or((0.95, 1.2), EmbedFace::line_metrics)
    }

    /// The width of `s` in this item's font and size.
    fn width_of(&self, face: Option<&EmbedFace>, s: &str) -> f64 {
        match face {
            Some(f) => f.shape(s).width(self.size),
            None => self.family.width(s, self.size, self.bold, self.italic),
        }
    }
}

impl Default for AddedText {
    fn default() -> Self {
        AddedText {
            rect: [0.0; 4],
            text: String::new(),
            family: Family::Helvetica,
            bold: false,
            italic: false,
            size: 12.0,
            color: [0.0; 3],
            align: Align::Left,
            font: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AddedImage {
    /// Display space.
    pub rect: [f64; 4],
    /// The image XObject.
    pub image: ObjRef,
    /// Quarter turns counter-clockwise (0–3).
    pub rotation: u8,
    pub flip_h: bool,
    pub flip_v: bool,
    /// The fraction trimmed from each side of the image: left, bottom, right, top.
    pub crop: [f64; 4],
}

impl AddedImage {
    pub fn new(rect: [f64; 4], image: ObjRef) -> Self {
        AddedImage { rect, image, rotation: 0, flip_h: false, flip_v: false, crop: [0.0; 4] }
    }

    /// The matrix from image space (the unit square) to display space: crop, flip, rotate,
    /// then fill the box.
    fn matrix(&self) -> pdfcraft_content::Matrix {
        use pdfcraft_content::Matrix as M;
        let [l, b, r, t] = self.crop.map(|v| v.clamp(0.0, 0.45));
        let (cw, ch) = ((1.0 - l - r).max(0.05), (1.0 - b - t).max(0.05));
        let mut m = M([1.0 / cw, 0.0, 0.0, 1.0 / ch, -l / cw, -b / ch]);
        if self.flip_h {
            m = m.then(&M([-1.0, 0.0, 0.0, 1.0, 1.0, 0.0]));
        }
        if self.flip_v {
            m = m.then(&M([1.0, 0.0, 0.0, -1.0, 0.0, 1.0]));
        }
        for _ in 0..self.rotation % 4 {
            // A quarter turn counter-clockwise within the unit square.
            m = m.then(&M([0.0, 1.0, -1.0, 0.0, 1.0, 0.0]));
        }
        let [x0, y0, x1, y1] = norm(self.rect);
        m.then(&M([x1 - x0, 0.0, 0.0, y1 - y0, x0, y0]))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Content {
    Text(AddedText),
    Image(AddedImage),
}

impl Content {
    pub fn rect(&self) -> [f64; 4] {
        match self {
            Content::Text(t) => t.rect,
            Content::Image(i) => i.rect,
        }
    }

    /// The same item with its box and text size scaled by `factor`.
    fn scaled(self, factor: f64) -> Content {
        match self {
            _ if factor == 1.0 => self,
            Content::Text(t) => Content::Text(AddedText { rect: t.rect.map(|v| v * factor), size: t.size * factor, ..t }),
            Content::Image(i) => Content::Image(AddedImage { rect: i.rect.map(|v| v * factor), ..i }),
        }
    }

    /// The same item moved/resized to `rect` (text keeps its computed height).
    pub fn with_rect(&self, rect: [f64; 4]) -> Content {
        match self {
            Content::Text(t) => Content::Text(AddedText { rect, ..t.clone() }),
            Content::Image(i) => Content::Image(AddedImage { rect, ..i.clone() }),
        }
    }
}

/// An added item on a page.
#[derive(Clone, Debug, PartialEq)]
pub struct Added {
    pub page: usize,
    /// The item's content stream.
    pub obj: ObjRef,
    pub content: Content,
}

/// PdfKub-owned page content discovered in one page/content traversal.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MarkerScan {
    pub marks: Vec<MarkKind>,
    pub added: Vec<Added>,
}

fn nums(doc: &Document, o: Option<&Object>) -> Vec<f64> {
    o.map(|o| doc.resolve(o)).and_then(|a| a.as_array().map(|a| a.iter().filter_map(Object::as_f64).collect())).unwrap_or_default()
}

fn norm(r: [f64; 4]) -> [f64; 4] {
    [r[0].min(r[2]), r[1].min(r[3]), r[0].max(r[2]), r[1].max(r[3])]
}

/// The lines of a text item after wrapping to its box width.
pub fn lines(t: &AddedText) -> Vec<String> {
    wrapped(t).into_iter().map(|(line, _)| line).collect()
}

/// [`lines`], each with whether its paragraph runs right to left (its first strong character
/// does).
fn wrapped(t: &AddedText) -> Vec<(String, bool)> {
    let width = (t.rect[2] - t.rect[0]).max(t.size);
    // Text drawn with an embedded face (Thai, or a font picked from the list) wraps by the face's
    // shaped widths, left to right.
    if let Some(face) = t.face() {
        return pdfcraft_fonts::wrap_fitting(&t.text, |s| t.width_of(Some(&face), s) <= width).into_iter().map(|l| (l, false)).collect();
    }
    let mut out = Vec::new();
    for para in t.text.split('\n') {
        // Arabic or Hebrew: laid out with the bidirectional algorithm and shaped.
        let rtl_script = para.chars().any(|c| is_arabic(c) || is_hebrew(c));
        let thai = para.chars().any(is_thai);
        if thai
            && !rtl_script
            && let Ok(lines) = thai_lines(t, para, width)
        {
            out.extend(lines.into_iter().map(|line| (line, false)));
            continue;
        }
        let rtl = rtl_script && unicode_bidi::get_base_direction(para) == unicode_bidi::Direction::Rtl;
        // Arabic words are shaped one by one (joining stops at spaces), so a line's width is the
        // sum of its words' and shaping stays linear in the paragraph's length. The space is the
        // paragraph's; a space between Latin words is drawn in the item's font, a few hundredths
        // of an em apart.
        let measure = |s: &str| if rtl_script || thai { shaped_width(t, s, rtl) } else { t.family.width(s, t.size, t.bold, t.italic) };
        let space = measure(" ");
        let (mut line, mut line_w) = (String::new(), 0.0);
        for word in para.split(' ') {
            let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            let candidate_w =
                if rtl_script || thai { if line.is_empty() { measure(word) } else { line_w + space + measure(word) } } else { measure(&candidate) };
            if !line.is_empty() && candidate_w > width {
                out.push((std::mem::take(&mut line), rtl));
                line = word.to_string();
                line_w = measure(word);
            } else {
                line = candidate;
                line_w = candidate_w;
            }
        }
        out.push((line, rtl));
    }
    out
}

/// Thai wrapping at shaped cluster boundaries never detaches a tone/vowel mark from its base.
/// This is glyph-safe wrapping, not dictionary-based Thai word segmentation.
fn thai_lines(t: &AddedText, para: &str, width: f64) -> Result<Vec<String>, EditError> {
    let (pieces, _) = shaped_layout(t, para, false)?;
    let mut atoms = Vec::new();
    for piece in pieces {
        match piece {
            Piece::Shaped(clusters) => atoms.extend(clusters.into_iter().map(|c| (c.text, c.advance * t.size))),
            Piece::Latin(s) => atoms.extend(s.chars().map(|c| {
                let text = c.to_string();
                let w = t.family.width(&text, t.size, t.bold, t.italic);
                (text, w)
            })),
        }
    }
    let mut lines = Vec::new();
    let (mut line, mut used) = (String::new(), 0.0);
    for (text, advance) in atoms {
        if !line.is_empty() && used + advance > width {
            lines.push(std::mem::take(&mut line));
            used = 0.0;
        }
        line.push_str(&text);
        used += advance;
    }
    lines.push(line);
    Ok(lines)
}

fn is_thai(c: char) -> bool {
    matches!(u32::from(c), 0x0E00..=0x0E7F)
}

/// Arabic letters, marks, digits and punctuation: an item with any of them is shaped with the
/// craft-fonts Arabic face.
fn is_arabic(c: char) -> bool {
    matches!(u32::from(c), 0x0600..=0x06FF | 0x0750..=0x077F | 0x0870..=0x08FF | 0xFB50..=0xFDFF | 0xFE70..=0xFEFC)
}

/// Hebrew letters, points, cantillation marks and punctuation (and the presentation forms): an
/// item with any of them is drawn with the craft-fonts Hebrew face.
fn is_hebrew(c: char) -> bool {
    matches!(u32::from(c), 0x0591..=0x05F4 | 0xFB1D..=0xFB4F)
}

/// The first character of `t` that would be drawn as `?`: outside WinAnsiEncoding and not drawn
/// with the craft-fonts Arabic (#125) or Hebrew (#128) face, or Arabic or Hebrew that the face can't
/// draw (or that has no face to draw it). Page-content tools refuse such text; the Add-text editor
/// keeps it open.
/// Text drawn with an embedded face (PdfKub: Thai in Sarabun, or a picked font) is checked
/// against that face instead.
pub fn first_undrawable(t: &AddedText) -> Option<char> {
    if let Some(face) = t.face() {
        // Lines split on LF only: any other control character would be drawn as a missing glyph.
        let missing = |c: char| !c.is_whitespace() && !face.covers(c.encode_utf8(&mut [0; 4]));
        return t.text.split('\n').flat_map(str::chars).find(|&c| c.is_control() || missing(c));
    }
    if t.text.chars().any(is_thai) {
        if let Some(c) = t.text.chars().find(|c| is_thai(*c) && !pdfcraft_fonts::thai_has(*c)) {
            return Some(c);
        }
        let rest = AddedText { text: t.text.chars().filter(|c| !is_thai(*c)).collect(), ..t.clone() };
        return first_undrawable(&rest);
    }
    // Arabic or Hebrew that nothing can draw: there's no face, or the face lacks it.
    if let Some(c) =
        t.text.chars().find(|c| (shaped_arabic(*c) && !pdfcraft_fonts::arabic_has(*c)) || (is_hebrew(*c) && !pdfcraft_fonts::hebrew_has(*c)))
    {
        return Some(c);
    }
    if !shaped_by_a_face(t) {
        // Drawn line by line in the item's standard font (`\n` splits lines). Without the face
        // that is also how `draw` redraws an existing item, so it's checked that way.
        return t.text.split('\n').find_map(pdfcraft_fonts::first_non_win_ansi);
    }
    // What `shaped_layout` leaves to the standard font, exactly as `draw_shaped` splits it.
    wrapped(t).into_iter().find_map(|(line, rtl)| {
        let (pieces, _) = shaped_layout(t, &line, rtl).ok()?;
        pieces.iter().find_map(|p| match p {
            Piece::Latin(s) => pdfcraft_fonts::first_non_win_ansi(s),
            Piece::Shaped(_) => None,
        })
    })
}

/// Whether `t` has Arabic or Hebrew that its craft-fonts face draws (so `draw` lays it out with
/// [`draw_shaped`]).
fn shaped_by_a_face(t: &AddedText) -> bool {
    (pdfcraft_fonts::document_arabic_font().is_some() && t.text.chars().any(is_arabic))
        || (pdfcraft_fonts::document_hebrew_font(false).is_some() && t.text.chars().any(is_hebrew))
}

/// An Arabic character the face shapes and draws (U+061C, a direction mark, is dropped).
fn shaped_arabic(c: char) -> bool {
    is_arabic(c) && !is_bidi_control(c)
}

/// Direction marks, embeddings, overrides and isolates: they steer the order and are not drawn.
fn is_bidi_control(c: char) -> bool {
    matches!(c, '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

fn mirrored(c: char) -> char {
    match c {
        '(' => ')',
        ')' => '(',
        '[' => ']',
        ']' => '[',
        '{' => '}',
        '}' => '{',
        '<' => '>',
        '>' => '<',
        '«' => '»',
        '»' => '«',
        other => other,
    }
}

/// A piece of a line with shaped text, in drawing order.
enum Piece {
    /// Shown in the item's own standard font.
    Latin(String),
    Shaped(Vec<ShapedCluster>),
}

/// A line of a shaped-text item in drawing order (the Unicode bidirectional algorithm, UAX #9), and
/// its width at the item's size. `rtl` is its paragraph's direction.
fn shaped_layout(t: &AddedText, line: &str, rtl: bool) -> Result<(Vec<Piece>, f64), EditError> {
    // unicode-bidi indexes the first level of a line, so an empty one would panic.
    if line.is_empty() {
        return Ok((Vec::new(), 0.0));
    }
    let base = if rtl { Level::rtl() } else { Level::ltr() };
    let info = ParagraphBidiInfo::new(line, Some(base));
    let (levels, runs) = info.visual_runs(0..line.len());
    let mut pieces = Vec::new();
    for run in runs {
        let run_rtl = levels.get(run.start).is_some_and(Level::is_rtl);
        // Runs lie on character boundaries; a missing slice is skipped, never a panic.
        let Some(text) = line.get(run) else { continue };
        // Arabic and Hebrew letters take their face; so do spaces and punctuation between them
        // (Hebrew's when they follow Hebrew), which the shaper then mirrors. Latin text (and what
        // the faces lack) keeps the item's font.
        let mut segments: Vec<(Option<ShapedScript>, String)> = Vec::new();
        for c in text.chars().filter(|c| !is_bidi_control(*c)) {
            let after_hebrew = segments.last().is_some_and(|(kind, _)| *kind == Some(ShapedScript::Hebrew));
            let script = if is_thai(c) {
                Some(ShapedScript::Thai)
            } else if is_hebrew(c) || (run_rtl && after_hebrew && !c.is_alphanumeric() && pdfcraft_fonts::hebrew_has(c)) {
                Some(ShapedScript::Hebrew)
            } else if is_arabic(c) || (run_rtl && !c.is_alphanumeric() && pdfcraft_fonts::arabic_has(c)) {
                Some(ShapedScript::Arabic)
            } else {
                None
            };
            match segments.last_mut() {
                Some((kind, s)) if *kind == script => s.push(c),
                _ => segments.push((script, c.to_string())),
            }
        }
        if run_rtl {
            segments.reverse();
        }
        for (script, s) in segments {
            pieces.push(if let Some(script) = script {
                let shaped = match script {
                    ShapedScript::Arabic => shape_arabic(&s, run_rtl).map_err(|e| arabic_error(e, &s))?,
                    ShapedScript::Hebrew => shape_hebrew(&s, run_rtl).map_err(|e| hebrew_error(e, &s))?,
                    ShapedScript::Thai => shape_thai(&s).map_err(|e| thai_error(e, &s))?,
                };
                Piece::Shaped(shaped)
            } else if run_rtl {
                Piece::Latin(s.chars().rev().map(mirrored).collect())
            } else {
                Piece::Latin(s)
            });
        }
    }
    let width = pieces
        .iter()
        .map(|p| match p {
            Piece::Latin(s) => t.family.width(s, t.size, t.bold, t.italic),
            Piece::Shaped(clusters) => clusters.iter().map(|c| c.advance).sum::<f64>() * t.size,
        })
        .sum();
    Ok((pieces, width))
}

/// The width of `s` in a shaped-text item; the item's font's estimate if it can't be shaped (drawing
/// then reports why).
fn shaped_width(t: &AddedText, s: &str, rtl: bool) -> f64 {
    shaped_layout(t, s, rtl).map(|(_, w)| w).unwrap_or_else(|_| t.family.width(s, t.size, t.bold, t.italic))
}

fn arabic_error(e: GlyphError, text: &str) -> EditError {
    EditError::Invalid(match e {
        GlyphError::NoFont => "Arabic text needs PdfKub's Arabic font, which this build doesn't include \
                               (to build it in, set CRAFT_FONTS_DIR to a craft-fonts checkout that has an Arab face)"
            .into(),
        GlyphError::Missing => format!("the Arabic font can't show \"{text}\""),
        GlyphError::TooComplex => format!("the Arabic font's glyphs for \"{text}\" are too complex"),
    })
}

fn hebrew_error(e: GlyphError, text: &str) -> EditError {
    EditError::Invalid(match e {
        GlyphError::NoFont => "Hebrew text needs PdfKub's Hebrew font, which this build doesn't include \
                               (to build it in, set CRAFT_FONTS_DIR to a craft-fonts checkout that has a Hebr face)"
            .into(),
        GlyphError::Missing => format!("the Hebrew font can't show \"{text}\""),
        GlyphError::TooComplex => format!("the Hebrew font's glyphs for \"{text}\" are too complex"),
    })
}

fn thai_error(e: GlyphError, text: &str) -> EditError {
    EditError::Invalid(match e {
        GlyphError::NoFont => {
            "Thai text needs a Thai font in this build; rebuild with CRAFT_FONTS_DIR pointing to craft-fonts with a Thai face".into()
        }
        GlyphError::Missing => format!("the Thai font can't show \"{text}\""),
        GlyphError::TooComplex => "the Thai glyphs are too complex; use a shorter text box".into(),
    })
}

/// The box a text item occupies (height from its lines).
pub fn text_rect(t: &AddedText) -> [f64; 4] {
    let r = norm(t.rect);
    let (_, step) = t.spacing(t.face().as_ref());
    let h = lines(t).len().max(1) as f64 * t.size * step;
    [r[0], r[3] - h, r[2], r[3]]
}

/// The widest explicit line before wrapping, measured with the PDF writing fonts.
pub fn text_width(t: &AddedText) -> f64 {
    t.text.split('\n').map(|line| shaped_width(t, line, unicode_bidi::get_base_direction(line) == unicode_bidi::Direction::Rtl)).fold(0.0, f64::max)
}

fn font_name(base: &str) -> String {
    format!("PCF{}", base.replace('-', ""))
}

/// The embedded Type0 font of a text item and each line's character codes.
struct EmbeddedText {
    font: ObjRef,
    codes: Vec<Vec<u16>>,
}

/// Most glyphs in one Type 3 font: its codes are single bytes, 1–240. An item gets one such font
/// per 240 distinct glyphs it needs.
const GLYPHS_PER_FONT: usize = 240;

/// A line of a shaped-text item after layout: its text, whether its paragraph runs right to left,
/// its pieces in drawing order and its width at the item's size.
type LaidLine = (String, bool, Vec<Piece>, f64);

/// Draw a text item that has Arabic, Hebrew or Thai in it. Each line is laid out in display order; their glyphs
/// come from Type 3 fonts drawn from the craft-fonts face's outlines (no font file is embedded),
/// with a ToUnicode map for copy and search. Other text uses the item's standard font.
fn draw_shaped(doc: &mut Document, t: &AddedText, taken: &Dict, fonts: &mut Dict, out: &mut Vec<u8>) -> Result<(), EditError> {
    let laid = wrapped(t)
        .into_iter()
        .map(|(line, rtl)| shaped_layout(t, &line, rtl).map(|(pieces, w)| (line, rtl, pieces, w)))
        .collect::<Result<Vec<_>, _>>()?;
    draw_laid_shaped(doc, t, &laid, taken, fonts, out, shaped_glyph)
}

/// [`draw_shaped`] for lines that are already laid out. `outline` is the source of glyph outlines
/// for a script's face (the craft-fonts faces, or a stand-in in tests).
fn draw_laid_shaped(
    doc: &mut Document,
    t: &AddedText,
    laid: &[LaidLine],
    taken: &Dict,
    fonts: &mut Dict,
    out: &mut Vec<u8>,
    outline: impl Fn(ShapedScript, u32) -> Result<GlyphOutline, GlyphError>,
) -> Result<(), EditError> {
    let r = text_rect(t);
    let latin = font_name(t.family.base_font(t.bold, t.italic));
    // Every distinct cluster (a letter with its dots and the text it stands for) is one glyph of
    // one of the fonts, so copy and search get each character exactly once.
    let mut keys: Vec<&ShapedCluster> = Vec::new();
    let mut index: HashMap<ClusterKey, usize> = HashMap::new();
    for c in laid.iter().flat_map(|(_, _, pieces, _)| pieces).flat_map(|p| match p {
        Piece::Shaped(clusters) => clusters.as_slice(),
        Piece::Latin(_) => &[],
    }) {
        index.entry(cluster_key(c)).or_insert_with(|| {
            keys.push(c);
            keys.len() - 1
        });
    }

    // Every outline is read before anything is written, so a failure leaves the document as it was.
    let mut outlines = HashMap::new();
    for c in &keys {
        for (id, _) in &c.glyphs {
            if let std::collections::hash_map::Entry::Vacant(entry) = outlines.entry((c.script, *id)) {
                entry.insert(outline(c.script, *id).map_err(|e| match c.script {
                    ShapedScript::Arabic => arabic_error(e, &c.text),
                    ShapedScript::Hebrew => hebrew_error(e, &c.text),
                    ShapedScript::Thai => thai_error(e, &c.text),
                })?);
            }
        }
    }
    // The names the page and this item already use: a new font never takes one of them.
    let mut taken_names: HashSet<Vec<u8>> = taken.iter().chain(fonts.iter()).map(|(name, _)| name.clone()).collect();
    let mut names = Vec::new();
    for chunk in keys.chunks(GLYPHS_PER_FONT) {
        let font = type3_shaped(doc, chunk, &outlines);
        // A name no font on the page has, so items never replace each other's fonts (object
        // numbers alone don't do: a full save renumbers objects but keeps resource names).
        let mut name = format!("PCAr{}", font.num);
        let mut suffix = 0usize;
        while taken_names.contains(name.as_bytes()) {
            suffix = suffix.saturating_add(1);
            name = format!("PCAr{}_{suffix}", font.num);
        }
        taken_names.insert(name.clone().into_bytes());
        fonts.set(name.clone().into_bytes(), Object::Ref(font));
        names.push(name);
    }
    let [cr, cg, cb] = t.color.map(|v| v.clamp(0.0, 1.0));
    out.extend(format!("BT {} {} {} rg\n", n(cr), n(cg), n(cb)).bytes());
    for (i, (line, rtl, pieces, w)) in laid.iter().enumerate() {
        // Shaped glyphs are painted in visual order. Keep the original line's typing order
        // explicitly, including marks, ligatures, digits and mixed-script punctuation.
        let logical: String = line.encode_utf16().map(|u| format!("{u:04X}")).collect();
        out.extend(format!("/Span <</ActualText <FEFF{logical}>>> BDC\n").bytes());
        let spaces = line.matches(' ').count();
        // Justified: the spaces of every line but the last stretch to fill the box.
        let tw = if t.align == Align::Justify && i + 1 < laid.len() && spaces > 0 { ((r[2] - r[0]) - w).max(0.0) / spaces as f64 } else { 0.0 };
        let x = match t.align {
            // A right-to-left paragraph's unstretched last line ends at the right edge.
            Align::Justify if *rtl => r[2] - w - tw * spaces as f64,
            Align::Left | Align::Justify => r[0],
            Align::Center => r[0] + ((r[2] - r[0]) - w) / 2.0,
            Align::Right => r[2] - w,
        };
        let y = r[3] - (i as f64 * 1.2 + 0.95) * t.size;
        let mut pen = x;
        let mut current: Option<&str> = None;
        for piece in pieces {
            match piece {
                Piece::Latin(s) => {
                    out.extend(format!("/{latin} {} Tf {} Tw 1 0 0 1 {} {} Tm ", n(t.size), n(tw), n(pen), n(y)).bytes());
                    out.extend(literal(&win_ansi(s)));
                    out.extend_from_slice(b" Tj\n");
                    current = Some(latin.as_str());
                    pen += t.family.width(s, t.size, t.bold, t.italic) + tw * s.matches(' ').count() as f64;
                }
                Piece::Shaped(clusters) => {
                    for c in clusters {
                        let Some(&k) = index.get(&cluster_key(c)) else { continue };
                        let Some(name) = names.get(k / GLYPHS_PER_FONT) else { continue };
                        // Codes start at 1; k % 240 + 1 is at most 240.
                        let code = u8::try_from(k % GLYPHS_PER_FONT + 1).unwrap_or(1);
                        if current != Some(name.as_str()) {
                            out.extend(format!("/{name} {} Tf ", n(t.size)).bytes());
                            current = Some(name.as_str());
                        }
                        out.extend(format!("1 0 0 1 {} {} Tm ", n(pen), n(y)).bytes());
                        out.extend(literal(&[code]));
                        out.extend_from_slice(b" Tj\n");
                        pen += c.advance * t.size + if c.text == " " { tw } else { 0.0 };
                    }
                }
            }
        }
        out.extend_from_slice(b"EMC\n");
    }
    out.extend_from_slice(b"ET\n");
    Ok(())
}

/// What makes two clusters the same Type 3 glyph: text, glyphs, offsets and advance (bit for bit;
/// equal clusters come from the same font units).
type ClusterKey<'a> = (ShapedScript, &'a str, Vec<(u32, u64, u64)>, u64);

fn cluster_key(c: &ShapedCluster) -> ClusterKey<'_> {
    (c.script, c.text.as_str(), c.glyphs.iter().map(|(id, [x, y])| (*id, x.to_bits(), y.to_bits())).collect(), c.advance.to_bits())
}

/// A Type 3 font with `clusters` as codes 1…, each drawn from its script face's glyph `outlines`.
fn type3_shaped(doc: &mut Document, clusters: &[&ShapedCluster], outlines: &HashMap<(ShapedScript, u32), GlyphOutline>) -> ObjRef {
    let mut charprocs = Dict::new();
    let mut differences = vec![Object::Int(1)];
    let mut widths = Vec::with_capacity(clusters.len());
    let mut bbox = [0.0f64; 4];
    let mut to_unicode = Vec::new();
    for (i, c) in clusters.iter().enumerate() {
        let code = i + 1;
        // Glyph space is 1000 units per em.
        let mut paths = Vec::new();
        let mut ink: Option<[f64; 4]> = None;
        for (id, [dx, dy]) in &c.glyphs {
            let Some(g) = outlines.get(&(c.script, *id)) else { continue };
            for contour in &g.contours {
                for (j, p) in contour.iter().enumerate() {
                    paths.extend(format!("{} {} {}\n", n((p[0] + dx) * 1000.0), n((p[1] + dy) * 1000.0), if j == 0 { "m" } else { "l" }).bytes());
                }
                paths.extend_from_slice(b"h\n");
                let b = [g.bbox[0] + dx, g.bbox[1] + dy, g.bbox[2] + dx, g.bbox[3] + dy];
                ink = Some(ink.map_or(b, |a| [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]));
            }
        }
        // Rounded outwards so no ink is clipped.
        let [x0, y0, x1, y1] =
            ink.map_or([0.0; 4], |b| [(b[0] * 1000.0).floor(), (b[1] * 1000.0).floor(), (b[2] * 1000.0).ceil(), (b[3] * 1000.0).ceil()]);
        bbox = [bbox[0].min(x0), bbox[1].min(y0), bbox[2].max(x1), bbox[3].max(y1)];
        let mut proc = format!("{} 0 {} {} {} {} d1\n", n(c.advance * 1000.0), n(x0), n(y0), n(x1), n(y1)).into_bytes();
        if !paths.is_empty() {
            proc.extend(paths);
            proc.extend_from_slice(b"f\n");
        }
        let glyph = format!("g{code:02X}");
        charprocs.set(glyph.clone().into_bytes(), Object::Ref(doc.add(Object::Stream(Stream::flate(Dict::new(), &proc)))));
        differences.push(Object::name(&glyph));
        widths.push(Object::Real((c.advance * 1000.0).round()));
        if !c.text.is_empty() {
            let hex: String = c.text.encode_utf16().map(|u| format!("{u:04X}")).collect();
            to_unicode.push(format!("<{code:02X}> <{hex}>\n"));
        }
    }
    let mut cmap = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CMapType 2 def\n1 begincodespacerange\n<01> <FF>\nendcodespacerange\n",
    );
    // A CMap block holds at most 100 entries.
    for block in to_unicode.chunks(100) {
        cmap.push_str(&format!("{} beginbfchar\n{}endbfchar\n", block.len(), block.concat()));
    }
    cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    let cmap = doc.add(Object::Stream(Stream::flate(Dict::new(), cmap.as_bytes())));
    // The face's family when every glyph comes from one script; without the face, the script's
    // name (as Arabic items were always named).
    let family = match clusters.first().map(|c| c.script).filter(|s| clusters.iter().all(|c| c.script == *s)) {
        Some(script) => script.font().map_or(
            match script {
                ShapedScript::Arabic => "Arabic",
                ShapedScript::Hebrew => "Hebrew",
                ShapedScript::Thai => "Thai",
            },
            |f| f.family,
        ),
        None => "PdfKub Shaped Text",
    };
    let mut descriptor = Dict::new();
    descriptor.set(b"Type".to_vec(), Object::name("FontDescriptor"));
    descriptor.set(b"FontName".to_vec(), Object::name(&family.replace(' ', "")));
    descriptor.set(b"FontFamily".to_vec(), Object::String(PdfString::literal(family.as_bytes().to_vec())));
    descriptor.set(b"Flags".to_vec(), Object::Int(4));
    descriptor.set(b"ItalicAngle".to_vec(), Object::Int(0));
    let mut encoding = Dict::new();
    encoding.set(b"Type".to_vec(), Object::name("Encoding"));
    encoding.set(b"Differences".to_vec(), Object::Array(differences));
    let mut font = Dict::new();
    font.set(b"Type".to_vec(), Object::name("Font"));
    font.set(b"Subtype".to_vec(), Object::name("Type3"));
    font.set(b"FontDescriptor".to_vec(), Object::Ref(doc.add(Object::Dict(descriptor))));
    font.set(b"FontBBox".to_vec(), Object::Array(bbox.iter().map(|v| Object::Real(*v)).collect()));
    font.set(
        b"FontMatrix".to_vec(),
        Object::Array(vec![Object::Real(0.001), Object::Int(0), Object::Int(0), Object::Real(0.001), Object::Int(0), Object::Int(0)]),
    );
    font.set(b"FirstChar".to_vec(), Object::Int(1));
    font.set(b"LastChar".to_vec(), Object::Int(clusters.len() as i64));
    font.set(b"Widths".to_vec(), Object::Array(widths));
    font.set(b"Encoding".to_vec(), Object::Dict(encoding));
    font.set(b"CharProcs".to_vec(), Object::Dict(charprocs));
    font.set(b"ToUnicode".to_vec(), Object::Ref(cmap));
    doc.add(Object::Dict(font))
}

/// Content and resources for an item; `view` maps display space to user space. `taken` holds the
/// page's font names. `existing`: the item is being replaced, so a build without the Arabic or
/// Hebrew face keeps drawing that text as `?`, as before it was supported, rather than refusing to
/// move it.
/// `embedded` is set for text drawn with an [`EmbedFace`].
fn draw(
    doc: &mut Document,
    c: &Content,
    view: [f64; 6],
    taken: &Dict,
    existing: bool,
    embedded: Option<&EmbeddedText>,
) -> Result<(Vec<u8>, Dict), EditError> {
    let mut res = Dict::new();
    let mut out = format!(
        "q {} {} {} {} {} {} cm\n",
        matrix_number(view[0]),
        matrix_number(view[1]),
        matrix_number(view[2]),
        matrix_number(view[3]),
        matrix_number(view[4]),
        matrix_number(view[5])
    )
    .into_bytes();
    match c {
        Content::Text(t) if t.face().is_some() => {
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
                let w = shaped.width(t.size);
                let x0 = match t.align {
                    Align::Left | Align::Justify => r[0],
                    Align::Center => r[0] + ((r[2] - r[0]) - w) / 2.0,
                    Align::Right => r[2] - w,
                };
                let y0 = r[3] - (i as f64 * step + first) * t.size;
                let spaces = line.matches(' ').count();
                let extra =
                    if t.align == Align::Justify && i + 1 < all.len() && spaces > 0 { ((r[2] - r[0]) - w).max(0.0) / spaces as f64 } else { 0.0 };
                // The exact text for search, copy and screen readers, whatever glyphs shaping chose.
                let units: String = line.encode_utf16().map(|u| format!("{u:04X}")).collect();
                out.extend(format!("/Span << /ActualText <FEFF{units}> >> BDC\n").bytes());
                let mut shift = 0.0;
                let mut last_cluster = usize::MAX;
                let codes = embedded.codes.get(i).ok_or_else(|| EditError::Invalid("the text changed while it was drawn".into()))?;
                for (g, code) in shaped.glyphs.iter().zip(codes) {
                    if g.cluster != last_cluster {
                        if last_cluster != usize::MAX && line.get(..g.cluster).is_some_and(|s| s.ends_with(' ')) {
                            shift += extra;
                        }
                        last_cluster = g.cluster;
                    }
                    let x = x0 + g.x * scale + shift;
                    let y = y0 + g.y * scale;
                    out.extend(format!("1 0 0 1 {} {} Tm <{code:04X}> Tj\n", n(x), n(y)).bytes());
                }
                out.extend_from_slice(b"EMC\n");
            }
            out.extend_from_slice(b"ET\n");
        }
        Content::Text(t) => {
            let base = t.family.base_font(t.bold, t.italic);
            let name = font_name(base);
            let mut font = Dict::new();
            font.set(b"Type".to_vec(), Object::name("Font"));
            font.set(b"Subtype".to_vec(), Object::name("Type1"));
            font.set(b"BaseFont".to_vec(), Object::name(base));
            if t.family != Family::Courier || !t.text.is_ascii() {
                font.set(b"Encoding".to_vec(), Object::name("WinAnsiEncoding"));
            }
            let mut fonts = Dict::new();
            fonts.set(name.clone().into_bytes(), Object::Dict(font));
            let face_or_new = |has: fn(char) -> bool, face: bool| t.text.chars().any(has) && (face || !existing);
            if t.text.chars().any(is_thai)
                || face_or_new(is_arabic, pdfcraft_fonts::document_arabic_font().is_some())
                || face_or_new(is_hebrew, pdfcraft_fonts::document_hebrew_font(false).is_some())
            {
                draw_shaped(doc, t, taken, &mut fonts, &mut out)?;
                res.set(b"Font".to_vec(), Object::Dict(fonts));
                out.extend_from_slice(b"Q\n");
                return Ok((out, res));
            }
            res.set(b"Font".to_vec(), Object::Dict(fonts));
            let r = text_rect(t);
            let [cr, cg, cb] = t.color.map(|v| v.clamp(0.0, 1.0));
            out.extend(format!("BT /{name} {} Tf {} {} {} rg\n", n(t.size), n(cr), n(cg), n(cb)).bytes());
            for (i, line) in lines(t).iter().enumerate() {
                let w = t.family.width(line, t.size, t.bold, t.italic);
                let x = match t.align {
                    Align::Left | Align::Justify => r[0],
                    Align::Center => r[0] + ((r[2] - r[0]) - w) / 2.0,
                    Align::Right => r[2] - w,
                };
                // Baseline: 0.8 em below the line top.
                let y = r[3] - (i as f64 * 1.2 + 0.95) * t.size;
                // Justified: word spacing makes every line but the last fill the box.
                let all = lines(t);
                let spaces = line.matches(' ').count();
                let tw =
                    if t.align == Align::Justify && i + 1 < all.len() && spaces > 0 { ((r[2] - r[0]) - w).max(0.0) / spaces as f64 } else { 0.0 };
                out.extend(format!("1 0 0 1 {} {} Tm {} Tw ", n(x), n(y), n(tw)).bytes());
                out.extend(literal(&win_ansi(line)));
                out.extend_from_slice(b" Tj\n");
            }
            out.extend_from_slice(b"ET\n");
        }
        Content::Image(i) => {
            if !matches!(&*doc.get(i.image), Object::Stream(s) if s.dict.name(b"Subtype") == Some(b"Image")) {
                return Err(EditError::Invalid("not an image".into()));
            }
            let r = norm(i.rect);
            let name = format!("PCImg{}", i.image.num);
            let mut xo = Dict::new();
            xo.set(name.clone().into_bytes(), Object::Ref(i.image));
            res.set(b"XObject".to_vec(), Object::Dict(xo));
            let [a, b, c, d, e, f] = i.matrix().0;
            // Clip to the box: the cropped-away parts stay hidden.
            out.extend(
                format!(
                    "{} {} {} {} re W n {} {} {} {} {} {} cm /{name} Do\n",
                    n(r[0]),
                    n(r[1]),
                    n(r[2] - r[0]),
                    n(r[3] - r[1]),
                    n(a),
                    n(b),
                    n(c),
                    n(d),
                    n(e),
                    n(f)
                )
                .bytes(),
            );
        }
    }
    out.extend_from_slice(b"Q\n");
    Ok((out, res))
}

fn params(c: &Content) -> Dict {
    let mut d = Dict::new();
    let arr = |v: &[f64]| Object::Array(v.iter().map(|x| Object::Real(*x)).collect());
    match c {
        Content::Text(t) => {
            d.set(b"Kind".to_vec(), Object::name("Text"));
            d.set(b"Text".to_vec(), PdfString::text(&t.text));
            d.set(b"Font".to_vec(), Object::name(t.family.base_font(t.bold, t.italic)));
            d.set(b"Size".to_vec(), Object::Real(t.size));
            d.set(b"Color".to_vec(), arr(&t.color));
            d.set(
                b"Align".to_vec(),
                Object::Int(match t.align {
                    Align::Left => 0,
                    Align::Center => 1,
                    Align::Right => 2,
                    Align::Justify => 3,
                }),
            );
            d.set(b"Rect".to_vec(), arr(&text_rect(t)));
            // A chosen embedded face: its name, the font object and the file's length (to share
            // one font object between items with the same face).
            if let Some(f) = &t.font {
                d.set(b"EmbedFont".to_vec(), PdfString::text(&f.name));
                d.set(b"EmbedLength".to_vec(), Object::Int(f.data.len() as i64));
            }
        }
        Content::Image(i) => {
            d.set(b"Kind".to_vec(), Object::name("Image"));
            d.set(b"Image".to_vec(), Object::Ref(i.image));
            d.set(b"Rect".to_vec(), arr(&norm(i.rect)));
            if i.rotation % 4 != 0 {
                d.set(b"Rotate".to_vec(), Object::Int(i64::from(i.rotation % 4) * 90));
            }
            if i.flip_h {
                d.set(b"FlipH".to_vec(), Object::Bool(true));
            }
            if i.flip_v {
                d.set(b"FlipV".to_vec(), Object::Bool(true));
            }
            if i.crop != [0.0; 4] {
                d.set(b"Crop".to_vec(), arr(&i.crop));
            }
        }
    }
    d
}

/// The embedded font object of a saved text item, if it has one.
fn font_obj(d: &Dict) -> Option<ObjRef> {
    d.get(b"EmbedFontObj").and_then(Object::as_ref)
}

fn parse(doc: &Document, d: &Dict) -> Option<Content> {
    let r = nums(doc, d.get(b"Rect"));
    let rect: [f64; 4] = r.try_into().ok()?;
    match d.name(b"Kind")? {
        b"Text" => {
            let (family, bold, italic) = Family::from_base(d.name(b"Font").unwrap_or(b"Helvetica"));
            let c = nums(doc, d.get(b"Color"));
            Some(Content::Text(AddedText {
                rect,
                text: d.get(b"Text").and_then(|t| doc.resolve(t).as_string().map(|s| s.to_text())).unwrap_or_default(),
                family,
                bold,
                italic,
                size: d.get(b"Size").and_then(Object::as_f64).unwrap_or(12.0),
                color: if c.len() == 3 { [c[0], c[1], c[2]] } else { [0.0; 3] },
                align: match d.int(b"Align") {
                    Some(1) => Align::Center,
                    Some(2) => Align::Right,
                    Some(3) => Align::Justify,
                    _ => Align::Left,
                },
                font: d
                    .get(b"EmbedFont")
                    .and_then(|n| doc.resolve(n).as_string().map(|s| s.to_text()))
                    .zip(font_obj(d))
                    .and_then(|(name, obj)| EmbedFace::read(doc, obj, &name)),
            }))
        }
        b"Image" => {
            let crop = nums(doc, d.get(b"Crop"));
            Some(Content::Image(AddedImage {
                rect,
                image: d.get(b"Image")?.as_ref()?,
                rotation: (d.int(b"Rotate").unwrap_or(0).rem_euclid(360) / 90) as u8,
                flip_h: matches!(d.get(b"FlipH"), Some(Object::Bool(true))),
                flip_v: matches!(d.get(b"FlipV"), Some(Object::Bool(true))),
                crop: crop.try_into().unwrap_or([0.0; 4]),
            }))
        }
        _ => None,
    }
}

fn with_resolved<T>(doc: &Document, object: &Object, f: impl FnOnce(&Object) -> T) -> T {
    match object {
        Object::Ref(reference) => {
            let resolved = doc.get(*reference);
            f(&resolved)
        }
        direct => f(direct),
    }
}

fn for_each_content_entry(doc: &Document, page: &pdfcraft_model::Page, mut visit: impl FnMut(&Object)) {
    let Some(contents) = page.dict.get(b"Contents") else { return };
    with_resolved(doc, contents, |resolved| match resolved {
        Object::Array(entries) => {
            for entry in entries {
                visit(entry);
            }
        }
        _ => visit(contents),
    });
}

/// Finds page marks and editable added content in one pass over the page content entries.
pub fn scan_markers(doc: &Document) -> MarkerScan {
    let mut scan = MarkerScan::default();
    for (pi, page) in page_list(doc).iter().enumerate() {
        for_each_content_entry(doc, page, |entry| {
            with_resolved(doc, entry, |resolved| {
                let Some(dict) = resolved.as_dict() else { return };
                let tag = dict.name(b"PCMark");
                let kind = if tag == Some(&b"HeaderFooter"[..]) {
                    Some(MarkKind::HeaderFooter)
                } else if tag == Some(&b"Watermark"[..]) {
                    Some(MarkKind::Watermark)
                } else if tag == Some(&b"Background"[..]) {
                    Some(MarkKind::Background)
                } else {
                    None
                };
                if let Some(kind) = kind
                    && !scan.marks.contains(&kind)
                {
                    scan.marks.push(kind);
                }

                let Some(obj) = Object::as_ref(entry) else { return };
                if tag != Some(TAG.as_bytes()) {
                    return;
                }
                let Some(params_object) = dict.get(b"PCAdded") else { return };
                with_resolved(doc, params_object, |resolved_params| {
                    let Some(params) = resolved_params.as_dict() else { return };
                    if let Some(content) = parse(doc, params) {
                        // Display points depend on the page's /UserUnit. An item records the one it was
                        // written for; none means 1, as for items written before display space followed
                        // /UserUnit.
                        let written = params
                            .get(b"UserUnit")
                            .and_then(Object::as_f64)
                            .filter(|u| u.is_finite() && *u >= 1.0)
                            .map_or(1.0, |u| u.min(pdfcraft_model::MAX_USER_UNIT));
                        scan.added.push(Added { page: pi, obj, content: content.scaled(page.user_unit(doc) / written) });
                    }
                });
            });
        });
    }
    scan
}

fn validate(c: &Content) -> Result<(), EditError> {
    let r = c.rect();
    if !r.iter().all(|v| v.is_finite()) {
        return Err(EditError::Invalid("invalid position".into()));
    }
    match c {
        Content::Text(t) => {
            if t.text.trim().is_empty() {
                return Err(EditError::Invalid("type some text first".into()));
            }
            if !(t.size.is_finite() && (1.0..=500.0).contains(&t.size)) {
                return Err(EditError::Invalid("the font size must be between 1 and 500 points".into()));
            }
            if (r[2] - r[0]).abs() < 1.0 {
                return Err(EditError::Invalid("the text box is too narrow".into()));
            }
            // PdfKub: text drawn with an embedded face (Thai in Sarabun, a picked font) is checked
            // against that face; everything else as upstream checks it.
            if let Some(face) = t.face() {
                if let Some(c) = first_undrawable(t) {
                    return Err(EditError::Invalid(format!("the font {} has no letters for \"{c}\" (U+{:04X})", face.name, u32::from(c))));
                }
                return Ok(());
            }
            if t.text.chars().any(is_thai) && pdfcraft_fonts::document_thai_font().is_none() {
                return Err(thai_error(GlyphError::NoFont, &t.text));
            }
            // Without the Arabic face, `draw` decides for the Arabic letters themselves (#403): a new
            // item is refused with what's missing, and one that already holds Arabic stays movable.
            // Everything else in it is drawn in the standard font, so it still has to fit.
            let undrawable = if t.text.chars().any(shaped_arabic) && pdfcraft_fonts::document_arabic_font().is_none() {
                t.text.split('\n').find_map(|line| {
                    line.chars()
                        .find(|c| !(shaped_arabic(*c) || is_thai(*c) && pdfcraft_fonts::thai_has(*c) || pdfcraft_fonts::win_ansi_encodable(*c)))
                })
            } else {
                first_undrawable(t)
            };
            if let Some(c) = undrawable {
                return Err(crate::undrawable(c));
            }
        }
        Content::Image(_) => {
            if (r[2] - r[0]).abs() < 1.0 || (r[3] - r[1]).abs() < 1.0 {
                return Err(EditError::Invalid("the image is too small".into()));
            }
        }
    }
    Ok(())
}

/// Write an item's stream (new, or replacing `obj`) and make its resources available to the page.
fn write(doc: &mut Document, page: usize, c: &Content, obj: Option<ObjRef>) -> Result<ObjRef, EditError> {
    validate(c)?;
    let all = page_list(doc);
    check(&[page], all.len())?;
    // An embedded face is written once per document and shared by every item that uses it.
    let embedded = match c {
        Content::Text(t) => match t.face() {
            Some(face) => {
                let font = match shared_font(doc, &face) {
                    Some(r) => r,
                    None => pdfcraft_fonts::embedded_font(doc, &face).map_err(|e| EditError::Invalid(e.to_string()))?,
                };
                let lines = lines(t);
                let shaped: Vec<_> = lines.iter().map(|l| face.shape(l)).collect();
                let runs: Vec<_> = lines.iter().map(String::as_str).zip(&shaped).collect();
                let codes = face.codes_for(doc, font, &runs).map_err(|e| EditError::Invalid(e.to_string()))?;
                Some(EmbeddedText { font, codes })
            }
            None => None,
        },
        Content::Image(_) => None,
    };
    let font = embedded.as_ref().map(|e| e.font);
    let all = page_list(doc);
    let p = &all[page];
    let view = p.view_matrix(doc);
    let user_unit = p.user_unit(doc);
    // The page gets its own copy of its resources with the item's fonts and images added, and
    // without the Arabic fonts of the version it replaces.
    let mut pres = p.dict.get(b"Resources").map(|r| doc.resolve(r)).and_then(|r| r.as_dict().cloned()).unwrap_or_default();
    if let Some(r) = obj {
        drop_fonts(doc, &mut pres, &own_fonts(doc, r));
    }
    let taken = pres.get(b"Font").map(|f| doc.resolve(f)).and_then(|f| f.as_dict().cloned()).unwrap_or_default();
    let (content, res) = draw(doc, c, view, &taken, obj.is_some(), embedded.as_ref())?;
    for (k, v) in res.iter() {
        let mut sub = pres.get(k).map(|s| doc.resolve(s)).and_then(|s| s.as_dict().cloned()).unwrap_or_default();
        for (n2, o) in v.as_dict().into_iter().flat_map(|d| d.iter()) {
            sub.set(n2.clone(), o.clone());
        }
        pres.set(k.clone(), Object::Dict(sub));
    }
    doc.update_dict(p.obj, |d| d.set(b"Resources".to_vec(), Object::Dict(pres)))?;
    let mut sd = Dict::new();
    sd.set(b"PCMark".to_vec(), Object::name(TAG));
    let mut pd = params(c);
    if user_unit != 1.0 {
        pd.set(b"UserUnit".to_vec(), Object::Real(user_unit));
    }
    if let (Some(f), Content::Text(t)) = (font, c) {
        pd.set(b"EmbedFontObj".to_vec(), Object::Ref(f));
        let face = t.face().map(|f| f.name).unwrap_or_default();
        pd.set(b"EmbedFace".to_vec(), PdfString::text(&face));
    }
    sd.set(b"PCAdded".to_vec(), Object::Dict(pd));
    let stream = Stream::flate(sd, &content);
    match obj {
        Some(r) => {
            doc.set(r, Object::Stream(stream));
            Ok(r)
        }
        None => {
            // place_tagged appends a new stream; recover its reference.
            let p = page_list(doc).swap_remove(page);
            place_tagged(doc, &p, TAG, content.clone(), false)?;
            let p = page_list(doc).swap_remove(page);
            let r = contents(doc, &p)?.last().and_then(Object::as_ref).ok_or_else(|| EditError::Invalid("could not add the content".into()))?;
            doc.set(r, Object::Stream(stream));
            Ok(r)
        }
    }
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

/// Every added item, page by page, in drawing order.
pub fn list_added(doc: &Document) -> Vec<Added> {
    scan_markers(doc).added
}

/// Add an item to a page (0-based). Returns its index among the page's added items.
pub fn add_content(doc: &mut Document, page: usize, c: &Content) -> Result<usize, EditError> {
    write(doc, page, c, None)?;
    Ok(list_added(doc).iter().filter(|a| a.page == page).count() - 1)
}

fn find(doc: &Document, page: usize, index: usize) -> Result<Added, EditError> {
    list_added(doc)
        .into_iter()
        .filter(|a| a.page == page)
        .nth(index)
        .ok_or_else(|| EditError::Invalid(format!("page {} has no added item {}", page + 1, index + 1)))
}

/// Replace item `index` of `page` (moved, resized, retyped or reformatted).
pub fn update_content(doc: &mut Document, page: usize, index: usize, c: &Content) -> Result<(), EditError> {
    let a = find(doc, page, index)?;
    if std::mem::discriminant(&a.content) != std::mem::discriminant(c) {
        return Err(EditError::Invalid("an item can't change between text and image".into()));
    }
    write(doc, page, c, Some(a.obj))?;
    Ok(())
}

/// Remove item `index` of `page`.
pub fn delete_content(doc: &mut Document, page: usize, index: usize) -> Result<(), EditError> {
    let a = find(doc, page, index)?;
    let p = page_list(doc).swap_remove(page);
    let list: Vec<Object> = contents(doc, &p)?.into_iter().filter(|o| o.as_ref() != Some(a.obj)).collect();
    let fonts = own_fonts(doc, a.obj);
    // Only the page's own resources carry an item's fonts (`write` gives the page a copy).
    let res = match p.dict.get(b"Resources") {
        Some(Object::Dict(d)) if !fonts.is_empty() => Some(d.clone()),
        _ => None,
    };
    let res = res.map(|mut r| {
        drop_fonts(doc, &mut r, &fonts);
        r
    });
    doc.update_dict(p.obj, |d| {
        d.set(b"Contents".to_vec(), Object::Array(list));
        if let Some(r) = res {
            d.set(b"Resources".to_vec(), Object::Dict(r));
        }
    })?;
    Ok(())
}

/// The Type 3 fonts an Arabic item's stream shows text with. Each item has its own (`draw_shaped`
/// names them so), so they go when the item is replaced or deleted.
fn own_fonts(doc: &Document, obj: ObjRef) -> Vec<Vec<u8>> {
    let Object::Stream(s) = &*doc.get(obj) else { return Vec::new() };
    let Ok(data) = s.decoded() else { return Vec::new() };
    data.split(u8::is_ascii_whitespace).filter_map(|w| w.strip_prefix(b"/")).filter(|w| w.starts_with(b"PCAr")).map(<[u8]>::to_vec).collect()
}

fn drop_fonts(doc: &Document, res: &mut Dict, names: &[Vec<u8>]) {
    if names.is_empty() {
        return;
    }
    let Some(mut fonts) = res.get(b"Font").map(|f| doc.resolve(f)).and_then(|f| f.as_dict().cloned()) else { return };
    for name in names {
        fonts.remove(name);
    }
    res.set(b"Font".to_vec(), Object::Dict(fonts));
}

#[cfg(test)]
mod arabic_tests;
