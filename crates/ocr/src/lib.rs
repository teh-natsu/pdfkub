//! pdfcraft-ocr — Scan & OCR ▸ Recognize text (L4).
//!
//! Recognition runs the ocrs engine (MIT/Apache-2.0) with its pre-trained models (CC-BY-SA-4.0,
//! fetched by `cargo xtask models`; see ATTRIBUTION.toml). The caller renders a page to pixels;
//! [`Ocr::recognize`] finds the words in them, and [`text_layer`] turns words placed in user space
//! into page content: invisible text (rendering mode 3) over each word, so the page becomes a
//! searchable image (Acrobat's "Searchable Image (Exact)": the image is left untouched).
//!
//! The ocrs models read the Latin alphabet (English and other languages written without accents).
//! Thai uses PaddleOCR's PP-OCRv5 Thai recogniser (Apache-2.0, `thai-recognition.onnx` with its
//! character list in `thai-recognition.yml`, also fetched by `cargo xtask models`): ocrs finds
//! the lines, and the Thai model reads each one (Thai, with Latin letters and digits).

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::path::{Path, PathBuf};

pub use pdfcraft_fonts::helvetica_width;

/// The model files, as named in ATTRIBUTION.toml.
pub const DETECTION_MODEL: &str = "text-detection.rten";
pub const RECOGNITION_MODEL: &str = "text-recognition.rten";
/// PaddleOCR's Thai line recogniser and its configuration (the character list).
pub const THAI_RECOGNITION_MODEL: &str = "thai-recognition.onnx";
pub const THAI_RECOGNITION_CONFIG: &str = "thai-recognition.yml";

/// The languages the models read (ISO 639-1). English is the Latin alphabet without accents;
/// Thai also reads Latin letters and digits. Thai needs the Thai model files.
pub const LANGUAGES: &[(&str, &str)] = &[("en", "English"), ("th", "ไทย (Thai)")];

#[derive(Debug, thiserror::Error)]
pub enum OcrError {
    /// Shown to end users (release packages ship the models; a source build fetches them with
    /// `cargo xtask models`). The UI catalogs key this exact text.
    #[error("the text recognition models are not installed (reinstall PdfKub, or set PDFKUB_MODELS to the folder that holds them)")]
    NoModels,
    #[error("loading {0}: {1}")]
    Load(String, String),
    #[error("text recognition failed: {0}")]
    Recognize(String),
    #[error("the image is empty")]
    EmptyImage,
    #[error("the Thai text recognition model is not installed (run `cargo xtask models`, or set PDFKUB_MODELS)")]
    NoThaiModel,
}

/// Where the two model files are.
#[derive(Clone, Debug, PartialEq)]
pub struct Models {
    pub detection: PathBuf,
    pub recognition: PathBuf,
    /// The Thai recogniser and its configuration, when installed.
    pub thai: Option<(PathBuf, PathBuf)>,
}

impl Models {
    /// The models in `dir`, if both files are there.
    pub fn in_dir(dir: &Path) -> Option<Models> {
        let thai = (dir.join(THAI_RECOGNITION_MODEL), dir.join(THAI_RECOGNITION_CONFIG));
        let thai = (thai.0.is_file() && thai.1.is_file()).then_some(thai);
        let m = Models { detection: dir.join(DETECTION_MODEL), recognition: dir.join(RECOGNITION_MODEL), thai };
        (m.detection.is_file() && m.recognition.is_file()).then_some(m)
    }

    /// Look for the models: `$PDFKUB_MODELS`, then where the release packages install them
    /// beside the executable ([`Models::dirs_beside_exe`]), then the source tree's `assets/models/`.
    pub fn find() -> Option<Models> {
        Self::search_dirs().iter().find_map(|d| Self::in_dir(d))
    }

    /// The directories [`Models::find`] looks in, in order.
    pub fn search_dirs() -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        if let Some(d) = std::env::var_os("PDFKUB_MODELS") {
            dirs.push(PathBuf::from(d));
        }
        if let Some(exe) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
            dirs.extend(Self::dirs_beside_exe(&exe));
        }
        dirs.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/models"));
        dirs
    }

    /// Where the release packages put the models, relative to the directory holding the
    /// executable (packaging/*):
    /// - `models/` beside it: the Windows MSI and portable zip;
    /// - `../Resources/models`: the macOS app bundle (`Contents/MacOS` → `Contents/Resources`);
    /// - `../share/pdfkub/models`: the FHS-style trees (`bin/` → `share/`) of the Linux
    ///   deb/rpm/tar.gz/AppImage/Flatpak and the FreeBSD tarball, wherever they are installed.
    pub fn dirs_beside_exe(exe_dir: &Path) -> Vec<PathBuf> {
        vec![exe_dir.join("models"), exe_dir.join("../Resources/models"), exe_dir.join("../share/pdfkub/models")]
    }
}

/// A recognised word: its text and its box in image pixels `[left, top, right, bottom]`.
#[derive(Clone, Debug, PartialEq)]
pub struct Word {
    pub text: String,
    pub rect: [f32; 4],
}

/// A recognised line of words, in reading order.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Line {
    pub words: Vec<Word>,
}

impl Line {
    pub fn text(&self) -> String {
        self.words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ")
    }
}

/// A loaded recogniser. Loading takes a moment; keep one and reuse it.
pub struct Ocr {
    engine: ocrs::OcrEngine,
    thai: Option<thai::Recognizer>,
}

impl Ocr {
    pub fn load(models: &Models) -> Result<Ocr, OcrError> {
        let load = |p: &Path| rten::Model::load_file(p).map_err(|e| OcrError::Load(p.display().to_string(), e.to_string()));
        let params = ocrs::OcrEngineParams {
            detection_model: Some(load(&models.detection)?),
            recognition_model: Some(load(&models.recognition)?),
            ..Default::default()
        };
        let engine = ocrs::OcrEngine::new(params).map_err(|e| OcrError::Load("ocr engine".into(), e.to_string()))?;
        let thai = match &models.thai {
            Some((model, config)) => Some(thai::Recognizer::load(model, config)?),
            None => None,
        };
        Ok(Ocr { engine, thai })
    }

    /// Load the models found by [`Models::find`].
    pub fn find() -> Result<Ocr, OcrError> {
        Self::load(&Models::find().ok_or(OcrError::NoModels)?)
    }

    /// Whether `language` (a code from [`LANGUAGES`]) can be read with the installed models.
    pub fn reads(&self, language: &str) -> bool {
        language != "th" || self.thai.is_some()
    }

    /// Recognise the text in an RGBA (or RGB, or grey) image, `width` × `height` pixels, in
    /// `language` (a code from [`LANGUAGES`]). Thai lines come back as one word per line.
    pub fn recognize_in(&self, pixels: &[u8], width: u32, height: u32, language: &str) -> Result<Vec<Line>, OcrError> {
        if language != "th" {
            return self.recognize(pixels, width, height);
        }
        let thai = self.thai.as_ref().ok_or(OcrError::NoThaiModel)?;
        if width == 0 || height == 0 || pixels.is_empty() {
            return Err(OcrError::EmptyImage);
        }
        let rgb = to_rgb(pixels, width, height);
        let err = |e: &dyn std::fmt::Display| OcrError::Recognize(e.to_string());
        let source = ocrs::ImageSource::from_bytes(&rgb, (width, height)).map_err(|e| err(&e))?;
        let input = self.engine.prepare_input(source).map_err(|e| err(&e))?;
        let found = self.engine.detect_words(&input).map_err(|e| err(&e))?;
        use rten_imageproc::BoundingRect as _;
        let mut out = Vec::new();
        for line in self.engine.find_text_lines(&input, &found) {
            // The line's box: the union of its words' boxes.
            let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
            for w in &line {
                let r = w.bounding_rect();
                b = [b[0].min(r.left()), b[1].min(r.top()), b[2].max(r.right()), b[3].max(r.bottom())];
            }
            if !(b[2] > b[0] && b[3] > b[1]) {
                continue;
            }
            // The Thai model reads a line best whole, but a very long one loses letters: split
            // it, between the pieces ocrs found, into runs at most MAX_RUN line-heights long.
            const MAX_RUN: f32 = 22.0;
            let line_h = b[3] - b[1];
            let mut runs: Vec<[f32; 4]> = Vec::new();
            for w in &line {
                let r = w.bounding_rect();
                let rect = [r.left(), r.top(), r.right(), r.bottom()];
                match runs.last_mut() {
                    Some(run) if rect[2] - run[0] <= MAX_RUN * line_h => {
                        *run = [run[0].min(rect[0]), run[1].min(rect[1]), run[2].max(rect[2]), run[3].max(rect[3])];
                    }
                    _ => runs.push(rect),
                }
            }
            let mut words = Vec::new();
            for rect in runs {
                // Read with the full line height, so marks above and below stay in.
                let rect = [rect[0], b[1], rect[2], b[3]];
                let text = thai.read_line(&rgb, width, height, rect)?;
                if !text.trim().is_empty() {
                    words.push(Word { text: text.trim().to_owned(), rect });
                }
            }
            if !words.is_empty() {
                out.push(Line { words });
            }
        }
        Ok(out)
    }

    /// Recognise the text in an RGBA (or RGB, or grey) image, `width` × `height` pixels.
    pub fn recognize(&self, pixels: &[u8], width: u32, height: u32) -> Result<Vec<Line>, OcrError> {
        if width == 0 || height == 0 || pixels.is_empty() {
            return Err(OcrError::EmptyImage);
        }
        // ocrs wants 1 or 3 channels.
        let rgb: std::borrow::Cow<[u8]> = if pixels.len() == (width * height * 4) as usize {
            pixels.as_chunks::<4>().0.iter().flat_map(|p| [p[0], p[1], p[2]]).collect::<Vec<u8>>().into()
        } else {
            pixels.into()
        };
        let err = |e: &dyn std::fmt::Display| OcrError::Recognize(e.to_string());
        let source = ocrs::ImageSource::from_bytes(&rgb, (width, height)).map_err(|e| err(&e))?;
        let input = self.engine.prepare_input(source).map_err(|e| err(&e))?;
        let found = self.engine.detect_words(&input).map_err(|e| err(&e))?;
        let lines = self.engine.find_text_lines(&input, &found);
        let read = self.engine.recognize_text(&input, &lines).map_err(|e| err(&e))?;
        use ocrs::TextItem;
        Ok(read
            .into_iter()
            .flatten()
            .map(|line| Line {
                words: line
                    .words()
                    .map(|w| {
                        let r = w.bounding_rect();
                        Word { text: w.to_string(), rect: [r.left() as f32, r.top() as f32, r.right() as f32, r.bottom() as f32] }
                    })
                    .filter(|w| !w.text.trim().is_empty())
                    .collect(),
            })
            .filter(|l| !l.words.is_empty())
            .collect())
    }
}

/// RGB bytes from RGBA, RGB or grey pixels.
fn to_rgb(pixels: &[u8], width: u32, height: u32) -> Vec<u8> {
    let n = width as usize * height as usize;
    if pixels.len() == n * 4 {
        pixels.as_chunks::<4>().0.iter().flat_map(|p| [p[0], p[1], p[2]]).collect()
    } else if pixels.len() == n {
        pixels.iter().flat_map(|&g| [g, g, g]).collect()
    } else {
        pixels.to_vec()
    }
}

mod thai {
    //! PaddleOCR PP-OCRv5 line recognition: a 48-pixel-high BGR crop normalised to [-1, 1] in, a
    //! probability per class and step out, decoded greedily (CTC). Class 0 is the blank, then
    //! the character list, then a space.

    use std::path::Path;

    use rten_tensor::NdTensor;
    use rten_tensor::prelude::*;

    use super::OcrError;

    const HEIGHT: usize = 48;
    /// Longest crop fed to the model (a very long line is squeezed rather than cut).
    const MAX_WIDTH: usize = 3200;

    pub struct Recognizer {
        model: rten::Model,
        chars: Vec<String>,
    }

    impl Recognizer {
        pub fn load(model: &Path, config: &Path) -> Result<Recognizer, OcrError> {
            let load_err = |p: &Path, e: &dyn std::fmt::Display| OcrError::Load(p.display().to_string(), e.to_string());
            let yml = std::fs::read_to_string(config).map_err(|e| load_err(config, &e))?;
            let chars = character_list(&yml);
            if chars.is_empty() {
                return Err(load_err(config, &"no character_dict"));
            }
            let model = rten::Model::load_file(model).map_err(|e| load_err(model, &e))?;
            Ok(Recognizer { model, chars })
        }

        /// Read the text in `rect` (`[left, top, right, bottom]` pixels) of an RGB image.
        pub fn read_line(&self, rgb: &[u8], width: u32, height: u32, rect: [f32; 4]) -> Result<String, OcrError> {
            // A little room around the detected box: detection boxes hug the ink, and the Thai
            // vowels and tone marks above and below the line need to stay in.
            let h = rect[3] - rect[1];
            let pad_x = h * 0.35;
            let pad_y = h * 0.25;
            let x0 = (rect[0] - pad_x).max(0.0);
            let y0 = (rect[1] - pad_y).max(0.0);
            let x1 = (rect[2] + pad_x).min(width as f32);
            let y1 = (rect[3] + pad_y).min(height as f32);
            let (cw, ch) = (x1 - x0, y1 - y0);
            if cw < 2.0 || ch < 2.0 {
                return Ok(String::new());
            }
            let out_w = ((cw / ch * HEIGHT as f32).round() as usize).clamp(HEIGHT / 2, MAX_WIDTH);
            let mut input = NdTensor::<f32, 4>::zeros([1, 3, HEIGHT, out_w]);
            let (w, hgt) = (width as usize, height as usize);
            let px = |x: usize, y: usize, c: usize| f32::from(rgb.get((y.min(hgt - 1) * w + x.min(w - 1)) * 3 + c).copied().unwrap_or(255));
            for oy in 0..HEIGHT {
                let sy = y0 + (oy as f32 + 0.5) * ch / HEIGHT as f32 - 0.5;
                let (ya, fy) = (sy.floor().max(0.0) as usize, (sy - sy.floor()).clamp(0.0, 1.0));
                for ox in 0..out_w {
                    let sx = x0 + (ox as f32 + 0.5) * cw / out_w as f32 - 0.5;
                    let (xa, fx) = (sx.floor().max(0.0) as usize, (sx - sx.floor()).clamp(0.0, 1.0));
                    // Bilinear, then BGR channel order as the model was trained with.
                    for (plane, c) in [(0, 2), (1, 1), (2, 0)] {
                        let v = px(xa, ya, c) * (1.0 - fx) * (1.0 - fy)
                            + px(xa + 1, ya, c) * fx * (1.0 - fy)
                            + px(xa, ya + 1, c) * (1.0 - fx) * fy
                            + px(xa + 1, ya + 1, c) * fx * fy;
                        input[[0, plane, oy, ox]] = (v / 255.0 - 0.5) / 0.5;
                    }
                }
            }
            let out = self.model.run_one(input.into(), None).map_err(|e| OcrError::Recognize(e.to_string()))?;
            let probs: NdTensor<f32, 3> = out.try_into().map_err(|e: rten::TryFromValueError| OcrError::Recognize(e.to_string()))?;
            let probs = probs.slice(0);
            Ok(self.decode(probs))
        }

        /// Greedy CTC: the best class at each step, repeats merged, blanks dropped.
        fn decode(&self, probs: rten_tensor::NdTensorView<f32, 2>) -> String {
            let classes = probs.size(1);
            let mut text = String::new();
            let mut last = 0usize;
            for t in 0..probs.size(0) {
                let mut best = (0usize, f32::MIN);
                for c in 0..classes {
                    let p = probs[[t, c]];
                    if p > best.1 {
                        best = (c, p);
                    }
                }
                let c = best.0;
                if c != 0 && c != last {
                    match self.chars.get(c - 1) {
                        Some(ch) => text.push_str(ch),
                        // The class after the list is the space.
                        None if c == self.chars.len() + 1 => text.push(' '),
                        None => {}
                    }
                }
                last = c;
            }
            text
        }
    }

    /// The `character_dict` list of a PaddleOCR `inference.yml`: one entry per `- ` line, plain
    /// or in single quotes (`''` is a quote).
    pub fn character_list(yml: &str) -> Vec<String> {
        let mut lines = yml.lines().skip_while(|l| !l.trim_start().starts_with("character_dict:"));
        lines.next();
        let mut out = Vec::new();
        for l in lines {
            let Some(item) = l.trim_start().strip_prefix("- ").or_else(|| (l.trim() == "-").then_some("")) else { break };
            let item = match item.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
                Some(inner) if item.len() >= 2 => inner.replace("''", "'"),
                _ => item.to_owned(),
            };
            out.push(item);
        }
        out
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn the_character_list_is_read_from_the_yaml() {
            let yml = "PostProcess:\n  name: CTCLabelDecode\n  character_dict:\n  - '!'\n  - ''''\n  - '\"'\n  - A\n  - ก\nOther: 1\n";
            assert_eq!(super::character_list(yml), ["!", "'", "\"", "A", "ก"]);
        }
    }
}

/// A word placed on a page, in PDF user space: the bottom-left corner of its box, and the
/// vectors along its bottom edge (`across`, left → right as read) and its left edge (`up`).
#[derive(Clone, Debug, PartialEq)]
pub struct PlacedWord {
    pub text: String,
    pub origin: [f64; 2],
    pub across: [f64; 2],
    pub up: [f64; 2],
}

impl PlacedWord {
    /// Place a pixel-space word with `to_user`, which maps an image pixel (x right, y down) to
    /// user space.
    pub fn place(word: &Word, to_user: impl Fn(f32, f32) -> [f64; 2]) -> PlacedWord {
        let [l, t, r, b] = word.rect;
        let o = to_user(l, b);
        let br = to_user(r, b);
        let tl = to_user(l, t);
        PlacedWord { text: word.text.clone(), origin: o, across: [br[0] - o[0], br[1] - o[1]], up: [tl[0] - o[0], tl[1] - o[1]] }
    }
}

/// Helvetica's ascender and descender (em fractions): a word box spans about this much.
const BOX_EM: f64 = 0.93;
const DESCENT: f64 = 0.21;

fn num(v: f64) -> String {
    let s = format!("{:.4}", if v.abs() < 1e-9 { 0.0 } else { v });
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

/// Page content that writes `words` as invisible text in `/PCHelv` (standard Helvetica,
/// WinAnsiEncoding), each word stretched to its box so selection and search highlight the
/// right place. Marked content `/OCR` so it can be told apart from the page's own text.
pub fn text_layer(words: &[PlacedWord]) -> Vec<u8> {
    text_layer_with(words, "", &[])
}

/// A word already encoded for an embedded font: its character codes and its width in ems.
#[derive(Clone, Debug, PartialEq)]
pub struct EncodedWord {
    pub codes: Vec<u16>,
    pub width: f64,
}

/// The embedded font's ascender and descender used to fit an encoded word's box (Sarabun's
/// typographic metrics).
const EMBED_BOX_EM: f64 = 1.3;
const EMBED_DESCENT: f64 = 0.232;

/// [`text_layer`] where `encoded[i]`, when present, draws word `i` with the embedded font
/// `font` (a Type0 font with 2-byte codes, e.g. Sarabun for Thai) instead of Helvetica.
pub fn text_layer_with(words: &[PlacedWord], font: &str, encoded: &[Option<EncodedWord>]) -> Vec<u8> {
    let mut out = b"/OCR BMC\nBT\n3 Tr\n/PCHelv 1 Tf\n".to_vec();
    for (i, w) in words.iter().enumerate() {
        if let Some(Some(e)) = encoded.get(i)
            && !font.is_empty()
        {
            let height = w.up[0].hypot(w.up[1]);
            if e.width <= 0.0 || height <= 0.0 {
                continue;
            }
            let em = [w.up[0] / EMBED_BOX_EM, w.up[1] / EMBED_BOX_EM];
            let ax = [w.across[0] / e.width, w.across[1] / e.width];
            let o = [w.origin[0] + em[0] * EMBED_DESCENT, w.origin[1] + em[1] * EMBED_DESCENT];
            let m: Vec<String> = [ax[0], ax[1], em[0], em[1], o[0], o[1]].iter().map(|v| num(*v)).collect();
            let hex: String = e.codes.iter().map(|c| format!("{c:04X}")).collect();
            out.extend(format!("/{font} 1 Tf {} Tm <{hex}> Tj /PCHelv 1 Tf\n", m.join(" ")).bytes());
            continue;
        }
        let width = helvetica_width(&w.text, 1.0);
        let height = w.up[0].hypot(w.up[1]);
        if width <= 0.0 || height <= 0.0 {
            continue;
        }
        // The em square: its height spans the box; its width is stretched to fit the word.
        let em = [w.up[0] / BOX_EM, w.up[1] / BOX_EM];
        let ax = [w.across[0] / width, w.across[1] / width];
        let o = [w.origin[0] + em[0] * DESCENT, w.origin[1] + em[1] * DESCENT];
        let m = [ax[0], ax[1], em[0], em[1], o[0], o[1]];
        let nums: Vec<String> = m.iter().map(|v| num(*v)).collect();
        out.extend_from_slice(nums.join(" ").as_bytes());
        out.extend_from_slice(b" Tm ");
        out.extend(pdfcraft_fonts::literal(&pdfcraft_fonts::win_ansi(&w.text)));
        out.extend_from_slice(b" Tj\n");
    }
    out.extend_from_slice(b"ET\nEMC\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_words_through_the_mapping() {
        // 2 pixels per point, page 612 × 792, y flipped.
        let to_user = |x: f32, y: f32| [x as f64 / 2.0, 792.0 - y as f64 / 2.0];
        let w = PlacedWord::place(&Word { text: "Hello".into(), rect: [100.0, 200.0, 300.0, 240.0] }, to_user);
        assert_eq!(w.origin, [50.0, 672.0]);
        assert_eq!(w.across, [100.0, 0.0]);
        assert_eq!(w.up, [0.0, 20.0]);
    }

    #[test]
    fn text_layer_is_invisible_text_fitted_to_each_box() {
        let w = PlacedWord { text: "Hi".into(), origin: [10.0, 20.0], across: [30.0, 0.0], up: [0.0, 9.3] };
        let s = String::from_utf8(text_layer(&[w])).unwrap();
        assert!(s.contains("3 Tr") && s.contains("/PCHelv 1 Tf"), "{s}");
        let width = helvetica_width("Hi", 1.0);
        assert!(s.contains(&format!("{} 0 0 10 10 22.1 Tm (Hi) Tj", num(30.0 / width))), "{s}");
        assert!(s.starts_with("/OCR BMC") && s.ends_with("EMC\n"));
    }

    #[test]
    fn rotated_pages_keep_the_reading_direction() {
        // A page turned 90°: image x runs up the page.
        let to_user = |x: f32, y: f32| [y as f64, x as f64];
        let w = PlacedWord::place(&Word { text: "Up".into(), rect: [0.0, 0.0, 50.0, 10.0] }, to_user);
        assert_eq!(w.across, [0.0, 50.0]);
        assert_eq!(w.up, [-10.0, 0.0]);
    }

    /// #103: every release package's layout puts the models where the installed app looks.
    /// Mirrors packaging/{windows/package.ps1,macos/package.sh,linux/package.sh,freebsd/package.sh}.
    #[test]
    fn finds_the_models_in_every_release_package_layout() {
        let base = std::env::temp_dir().join(format!("pdfcraft-ocr-layouts-{}", std::process::id()));
        let layouts = [
            ("windows", "PdfKub", "PdfKub/models"),
            ("macos", "PdfKub.app/Contents/MacOS", "PdfKub.app/Contents/Resources/models"),
            ("linux-deb-rpm", "usr/bin", "usr/share/pdfkub/models"),
            ("linux-tar", "pdfkub-1.0.0-linux-x86_64/bin", "pdfkub-1.0.0-linux-x86_64/share/pdfkub/models"),
            ("appimage", "PdfKub.AppDir/usr/bin", "PdfKub.AppDir/usr/share/pdfkub/models"),
            ("flatpak", "app/bin", "app/share/pdfkub/models"),
            ("freebsd", "usr/local/bin", "usr/local/share/pdfkub/models"),
        ];
        for (name, exe_dir, models_dir) in layouts {
            let root = base.join(name);
            let (exe_dir, models_dir) = (root.join(exe_dir), root.join(models_dir));
            std::fs::create_dir_all(&exe_dir).unwrap();
            std::fs::create_dir_all(&models_dir).unwrap();
            let found = || Models::dirs_beside_exe(&exe_dir).iter().find_map(|d| Models::in_dir(d));
            assert_eq!(found(), None, "{name}: no models yet");
            for file in [DETECTION_MODEL, RECOGNITION_MODEL] {
                std::fs::write(models_dir.join(file), b"model").unwrap();
            }
            let models = found().unwrap_or_else(|| panic!("{name}: models in {} not found", models_dir.display()));
            assert_eq!(models.detection.canonicalize().unwrap(), models_dir.join(DETECTION_MODEL).canonicalize().unwrap(), "{name}");
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The message tells end users what to do; release builds have no cargo.
    #[test]
    fn missing_models_message_is_for_end_users() {
        let message = OcrError::NoModels.to_string();
        assert!(!message.contains("cargo") && message.contains("PDFKUB_MODELS"), "{message}");
    }

    /// End to end on rendered text, when the models are installed (`cargo xtask models`).
    #[test]
    fn reads_rendered_text() {
        let Some(models) = Models::find() else {
            eprintln!("skipped: OCR models not installed");
            return;
        };
        let ocr = Ocr::load(&models).unwrap();
        let (w, h, px) = test_image::hello();
        let lines = ocr.recognize(&px, w, h).unwrap();
        let text: Vec<String> = lines.iter().map(Line::text).collect();
        assert!(text.iter().any(|t| t.contains("HELL") && t.contains("WOR")), "{text:?}");
        let word = lines.iter().flat_map(|l| &l.words).find(|w| w.text.contains("HELL")).unwrap();
        assert!(word.rect[0] >= 10.0 && word.rect[0] < 60.0, "{word:?}");
    }

    mod test_image {
        /// "HELLO WORLD" drawn with blocky 5×7 letters, 4 pixels per dot, black on white.
        pub fn hello() -> (u32, u32, Vec<u8>) {
            const GLYPHS: &[(char, [&str; 7])] = &[
                ('H', ["X...X", "X...X", "X...X", "XXXXX", "X...X", "X...X", "X...X"]),
                ('E', ["XXXXX", "X....", "X....", "XXXX.", "X....", "X....", "XXXXX"]),
                ('L', ["X....", "X....", "X....", "X....", "X....", "X....", "XXXXX"]),
                ('O', [".XXX.", "X...X", "X...X", "X...X", "X...X", "X...X", ".XXX."]),
                ('W', ["X...X", "X...X", "X...X", "X.X.X", "X.X.X", "XX.XX", "X...X"]),
                ('R', ["XXXX.", "X...X", "X...X", "XXXX.", "X.X..", "X..X.", "X...X"]),
                ('D', ["XXXX.", "X...X", "X...X", "X...X", "X...X", "X...X", "XXXX."]),
            ];
            let text = "HELLO WORLD";
            let (dot, x0, y0) = (4u32, 40u32, 40u32);
            let (w, h) = (x0 * 2 + text.len() as u32 * 6 * dot, y0 * 2 + 7 * dot);
            let mut px = vec![255u8; (w * h * 4) as usize];
            for (i, c) in text.chars().enumerate() {
                let Some((_, rows)) = GLYPHS.iter().find(|g| g.0 == c) else { continue };
                for (ry, row) in rows.iter().enumerate() {
                    for (rx, b) in row.bytes().enumerate() {
                        if b != b'X' {
                            continue;
                        }
                        for dy in 0..dot {
                            for dx in 0..dot {
                                let x = x0 + (i as u32 * 6 + rx as u32) * dot + dx;
                                let y = y0 + ry as u32 * dot + dy;
                                let o = ((y * w + x) * 4) as usize;
                                px[o..o + 3].copy_from_slice(&[0, 0, 0]);
                            }
                        }
                    }
                }
            }
            (w, h, px)
        }
    }
}
