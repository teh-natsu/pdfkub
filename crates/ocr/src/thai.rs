//! PdfKub: Thai text recognition. ocrs finds the lines; PaddleOCR's PP-OCRv5 Thai recogniser
//! (Apache-2.0, `thai-recognition.onnx` with its character list in `thai-recognition.yml`, fetched
//! by `cargo xtask models`) reads each one: Thai, with Latin letters and digits. The ocrs engine
//! reads with it when the language is `th` and the files are beside its models.
//!
//! The model: a 48-pixel-high BGR crop normalised to [-1, 1] in, a probability per class and step
//! out, decoded greedily (CTC). Class 0 is the blank, then the character list, then a space.

use std::path::Path;

use rten_imageproc::BoundingRect as _;
use rten_tensor::NdTensor;
use rten_tensor::prelude::*;

use super::{Line, OcrError, Word};

/// PaddleOCR's Thai line recogniser and its configuration (the character list).
pub const THAI_RECOGNITION_MODEL: &str = "thai-recognition.onnx";
pub const THAI_RECOGNITION_CONFIG: &str = "thai-recognition.yml";

const HEIGHT: usize = 48;
/// Longest crop fed to the model (a very long line is squeezed rather than cut).
const MAX_WIDTH: usize = 3200;
/// The Thai model reads a line best whole, but a very long one loses letters: a line is read in
/// runs at most this many line-heights long, split between the pieces ocrs found.
const MAX_RUN: f32 = 22.0;

/// Whether the Thai recogniser's files are beside the models [`crate::Models::find`] finds.
pub fn installed() -> bool {
    crate::Models::search_dirs()
        .iter()
        .any(|d| crate::Models::in_dir(d).is_some() && d.join(THAI_RECOGNITION_MODEL).is_file() && d.join(THAI_RECOGNITION_CONFIG).is_file())
}

pub struct Recognizer {
    model: rten::Model,
    chars: Vec<String>,
}

impl Recognizer {
    /// The Thai recogniser in `dir`, when both its files are there.
    pub fn in_dir(dir: &Path) -> Option<Result<Recognizer, OcrError>> {
        let (model, config) = (dir.join(THAI_RECOGNITION_MODEL), dir.join(THAI_RECOGNITION_CONFIG));
        (model.is_file() && config.is_file()).then(|| Recognizer::load(&model, &config))
    }

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
    fn read_line(&self, rgb: &[u8], width: u32, height: u32, rect: [f32; 4]) -> Result<String, OcrError> {
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
        if !(cw >= 2.0 && ch >= 2.0) {
            return Ok(String::new());
        }
        let out_w = ((cw / ch * HEIGHT as f32).round() as usize).clamp(HEIGHT / 2, MAX_WIDTH);
        let mut input = NdTensor::<f32, 4>::zeros([1, 3, HEIGHT, out_w]);
        let (w, hgt) = (width as usize, height as usize);
        let px = |x: usize, y: usize, c: usize| {
            f32::from(rgb.get((y.min(hgt.saturating_sub(1)) * w + x.min(w.saturating_sub(1))) * 3 + c).copied().unwrap_or(255))
        };
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
        Ok(self.decode(probs.slice(0)))
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

/// Read the Thai text of an RGB image whose lines `engine` finds. Each run of a line comes back as
/// one word.
pub(crate) fn recognize(engine: &ocrs::OcrEngine, thai: &Recognizer, rgb: &[u8], width: u32, height: u32) -> Result<Vec<Line>, OcrError> {
    let err = |e: &dyn std::fmt::Display| OcrError::Recognize(e.to_string());
    let source = ocrs::ImageSource::from_bytes(rgb, (width, height)).map_err(|e| err(&e))?;
    let input = engine.prepare_input(source).map_err(|e| err(&e))?;
    let found = engine.detect_words(&input).map_err(|e| err(&e))?;
    let mut out = Vec::new();
    for line in engine.find_text_lines(&input, &found) {
        // The line's box: the union of its words' boxes.
        let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for w in &line {
            let r = w.bounding_rect();
            b = [b[0].min(r.left()), b[1].min(r.top()), b[2].max(r.right()), b[3].max(r.bottom())];
        }
        if !(b[2] > b[0] && b[3] > b[1]) {
            continue;
        }
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
            let text = thai.read_line(rgb, width, height, rect)?;
            if !text.trim().is_empty() {
                words.push(Word::new(text.trim().to_owned(), rect, "ocrs"));
            }
        }
        if !words.is_empty() {
            out.push(Line { words });
        }
    }
    Ok(out)
}

/// The `character_dict` list of a PaddleOCR `inference.yml`: one entry per `- ` line, plain or in
/// single quotes (`''` is a quote).
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
