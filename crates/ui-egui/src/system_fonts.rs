//! The last-resort interface font: one face already installed on this machine.
//!
//! The embedded faces (Inter, egui's defaults, craft-fonts) come first in every family; this one
//! only draws characters none of them has, such as an Arabic file name in a build without
//! craft-fonts. It is read at runtime and never embedded or shipped (AGENTS.md §1.4), and
//! `PDFKUB_SYSTEM_FONTS=0` turns it off (published screenshots do).

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use egui::FontData;

/// Larger files are not read: a font path is still untrusted input.
const MAX_BYTES: u64 = 32 << 20;
/// Faces tried in a collection (`.ttc`).
const MAX_FACES: u32 = 16;
/// Arabic letter alef: the face must have it to be worth loading.
const PROBE: char = '\u{0627}';

/// Han in both Chinese forms and Japanese kana (简 體 語 ご): the CJK face must have them all, so
/// the language list's 简体中文, 繁體中文 and 日本語 draw in a build without craft-fonts.
const CJK_PROBES: [char; 4] = ['\u{7B80}', '\u{9AD4}', '\u{8A9E}', '\u{3054}'];

/// The installed fallback face, read once. `None` when it is turned off or no candidate fits.
pub fn fallback() -> Option<Arc<FontData>> {
    static CACHE: OnceLock<Option<Arc<FontData>>> = OnceLock::new();
    CACHE.get_or_init(load).clone()
}

/// Telugu letters (త ె ల గ): the language list's తెలుగు.
const TELUGU_PROBES: [char; 4] = ['\u{0C24}', '\u{0C46}', '\u{0C32}', '\u{0C17}'];

/// The installed Telugu fallback face (PdfKub), read once, like [`cjk_fallback`].
pub fn telugu_fallback() -> Option<Arc<FontData>> {
    static CACHE: OnceLock<Option<Arc<FontData>>> = OnceLock::new();
    CACHE.get_or_init(|| if turned_off() { None } else { telugu_candidates().iter().find_map(|path| read_with(path, &TELUGU_PROBES)) }).clone()
}

/// Well-known faces with Telugu, best first.
fn telugu_candidates() -> Vec<PathBuf> {
    if cfg!(windows) {
        let dir = std::env::var_os("WINDIR").or_else(|| std::env::var_os("SystemRoot")).map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
        ["Nirmala.ttc", "Nirmala.ttf", "gautami.ttf"].iter().map(|f| dir.join("Fonts").join(f)).collect()
    } else if cfg!(target_os = "macos") {
        [
            "/System/Library/Fonts/KohinoorTelugu.ttc",
            "/System/Library/Fonts/Supplemental/Telugu MN.ttc",
            "/System/Library/Fonts/Supplemental/Telugu Sangam MN.ttc",
        ]
        .iter()
        .map(PathBuf::from)
        .collect()
    } else {
        let files = ["truetype/noto/NotoSansTelugu-Regular.ttf", "noto/NotoSansTelugu-Regular.ttf", "google-noto/NotoSansTelugu-Regular.ttf"];
        ["/usr/share/fonts", "/usr/local/share/fonts"].iter().flat_map(|dir| files.iter().map(move |f| Path::new(dir).join(f))).collect()
    }
}

/// The installed CJK fallback face (PdfKub), read once, after [`fallback`] in every family.
/// `None` when system fonts are turned off or no candidate has every [`CJK_PROBES`] character.
pub fn cjk_fallback() -> Option<Arc<FontData>> {
    static CACHE: OnceLock<Option<Arc<FontData>>> = OnceLock::new();
    CACHE.get_or_init(|| if turned_off() { None } else { cjk_candidates().iter().find_map(|path| read_with(path, &CJK_PROBES)) }).clone()
}

fn turned_off() -> bool {
    std::env::var_os("PDFKUB_SYSTEM_FONTS").is_some_and(|v| v == "0")
}

fn load() -> Option<Arc<FontData>> {
    if turned_off() {
        return None;
    }
    candidates().iter().find_map(|path| read(path))
}

/// Well-known CJK faces that cover Simplified and Traditional Chinese and Japanese kana, best
/// first.
fn cjk_candidates() -> Vec<PathBuf> {
    if cfg!(windows) {
        let dir = std::env::var_os("WINDIR").or_else(|| std::env::var_os("SystemRoot")).map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
        ["msyh.ttc", "msjh.ttc", "simsun.ttc", "YuGothR.ttc", "meiryo.ttc"].iter().map(|f| dir.join("Fonts").join(f)).collect()
    } else if cfg!(target_os = "macos") {
        ["/System/Library/Fonts/PingFang.ttc", "/System/Library/Fonts/Hiragino Sans GB.ttc", "/System/Library/Fonts/STHeiti Light.ttc"]
            .iter()
            .map(PathBuf::from)
            .collect()
    } else {
        let files = [
            "opentype/noto/NotoSansCJK-Regular.ttc",
            "noto-cjk/NotoSansCJK-Regular.ttc",
            "google-noto-cjk/NotoSansCJK-Regular.ttc",
            "truetype/wqy/wqy-microhei.ttc",
            "wenquanyi/wqy-microhei/wqy-microhei.ttc",
        ];
        ["/usr/share/fonts", "/usr/local/share/fonts"].iter().flat_map(|dir| files.iter().map(move |f| Path::new(dir).join(f))).collect()
    }
}

/// Well-known locations of faces with broad script coverage, best first.
fn candidates() -> Vec<PathBuf> {
    if cfg!(windows) {
        let dir = std::env::var_os("WINDIR").or_else(|| std::env::var_os("SystemRoot")).map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
        ["segoeui.ttf", "tahoma.ttf", "arial.ttf"].iter().map(|f| dir.join("Fonts").join(f)).collect()
    } else if cfg!(target_os = "macos") {
        ["/System/Library/Fonts/SFArabic.ttf", "/System/Library/Fonts/GeezaPro.ttc", "/System/Library/Fonts/Supplemental/Arial.ttf"]
            .iter()
            .map(PathBuf::from)
            .collect()
    } else {
        let files = [
            "truetype/noto/NotoSansArabic-Regular.ttf",
            "noto/NotoSansArabic-Regular.ttf",
            "google-noto/NotoSansArabic-Regular.ttf",
            "truetype/dejavu/DejaVuSans.ttf",
            "TTF/DejaVuSans.ttf",
            "dejavu/DejaVuSans.ttf",
            "dejavu-sans-fonts/DejaVuSans.ttf",
        ];
        ["/usr/share/fonts", "/usr/local/share/fonts"].iter().flat_map(|dir| files.iter().map(move |f| Path::new(dir).join(f))).collect()
    }
}

fn read(path: &Path) -> Option<Arc<FontData>> {
    read_with(path, &[PROBE])
}

/// The first face of the font file at `path` that maps every character of `probes`.
fn read_with(path: &Path, probes: &[char]) -> Option<Arc<FontData>> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let index = face_with_all(&bytes, probes)?;
    let mut data = FontData::from_owned(bytes);
    data.index = index;
    log::info!("interface font fallback: {}", path.display());
    Some(Arc::new(data))
}

/// The first face of the file that parses and maps `c`. egui parses fonts with the same skrifa,
/// so a face accepted here is one it can load.
#[cfg(test)]
fn face_with(bytes: &[u8], c: char) -> Option<u32> {
    face_with_all(bytes, &[c])
}

/// [`face_with`] for every character of `probes`.
fn face_with_all(bytes: &[u8], probes: &[char]) -> Option<u32> {
    use skrifa::MetadataProvider as _;
    (0..MAX_FACES).find(|&index| {
        skrifa::FontRef::from_index(bytes, index).is_ok_and(|font| {
            let map = font.charmap();
            probes.iter().all(|&c| map.map(c).is_some())
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broken_and_missing_files_are_skipped() {
        assert_eq!(face_with(b"", PROBE), None);
        assert_eq!(face_with(b"not a font at all", PROBE), None);
        assert_eq!(face_with(&[0u8; 4096], PROBE), None);
        assert!(read(Path::new("definitely/not/here.ttf")).is_none());
        // A directory is not a font.
        assert!(read(&std::env::temp_dir()).is_none());
    }

    #[test]
    fn a_face_without_the_probe_is_rejected() {
        // Inter is Latin, Greek and Cyrillic only.
        let inter = include_bytes!("../../../assets/fonts/Inter-Regular.ttf");
        assert_eq!(face_with(inter, PROBE), None);
        assert_eq!(face_with(inter, 'A'), Some(0));
    }

    #[test]
    fn candidates_are_absolute_font_files() {
        let list = candidates();
        assert!(!list.is_empty());
        assert!(list.iter().all(|p| p.extension().is_some_and(|e| e == "ttf" || e == "ttc")));
        for list in [cjk_candidates(), telugu_candidates()] {
            assert!(!list.is_empty() && list.iter().all(|p| p.is_absolute() || cfg!(windows)));
        }
    }

    #[test]
    fn the_cjk_face_has_every_probe_or_is_none() {
        // Anuphan has Thai and Latin only.
        let anuphan = include_bytes!("../../../assets/fonts/Anuphan-Regular.ttf");
        assert_eq!(face_with_all(anuphan, &CJK_PROBES), None);
        assert_eq!(face_with_all(anuphan, &['A', '\u{0E01}']), Some(0));
        // On a machine with one of the candidates (Windows has Microsoft YaHei), it draws them.
        if let Some(data) = cjk_fallback() {
            assert_eq!(face_with_all(&data.font, &CJK_PROBES), Some(data.index));
        }
    }
}
