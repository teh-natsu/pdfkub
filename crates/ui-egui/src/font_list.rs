//! Fonts installed on this machine that added text can embed: TrueType faces whose licence allows
//! embedding (OS/2 `fsType`). Only each file's table directory and its `name` and `OS/2` tables
//! are read, so listing hundreds of fonts stays quick; a face is read in full when it's chosen.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use pdfcraft_engine::EmbedFace;

/// One face of an installed font file.
#[derive(Clone, Debug, PartialEq)]
pub struct SystemFace {
    /// The face's name in the font list and in the document ("TH Sarabun New Bold").
    pub name: String,
    pub path: PathBuf,
    /// The face's index in a collection (`.ttc`), else 0.
    pub index: u32,
    pub bold: bool,
    pub italic: bool,
}

impl SystemFace {
    /// Read the face for embedding.
    pub fn load(&self) -> Result<EmbedFace, String> {
        let meta = std::fs::metadata(&self.path).map_err(|e| e.to_string())?;
        if meta.len() > pdfcraft_fonts::MAX_FONT_BYTES as u64 {
            return Err(pdfcraft_engine::EmbedError::TooLarge.to_string());
        }
        let bytes = std::fs::read(&self.path).map_err(|e| e.to_string())?;
        EmbedFace::from_bytes(&self.name, &bytes, self.index).map_err(|e| e.to_string())
    }
}

/// An installed font family and its faces.
#[derive(Clone, Debug, PartialEq)]
pub struct SystemFamily {
    pub name: String,
    /// The family covers Thai (OS/2 Unicode range bit 24).
    pub thai: bool,
    pub faces: Vec<SystemFace>,
}

impl SystemFamily {
    /// The face closest to the requested style: an exact match, else the same weight, else the
    /// first face.
    pub fn face(&self, bold: bool, italic: bool) -> Option<&SystemFace> {
        self.faces
            .iter()
            .find(|f| f.bold == bold && f.italic == italic)
            .or_else(|| self.faces.iter().find(|f| f.bold == bold && !f.italic))
            .or_else(|| self.faces.iter().find(|f| !f.bold && !f.italic))
            .or_else(|| self.faces.first())
    }
}

/// The installed families, Thai ones first, each list sorted by name. Read once, on first use.
pub fn installed() -> &'static [SystemFamily] {
    static LIST: OnceLock<Vec<SystemFamily>> = OnceLock::new();
    LIST.get_or_init(scan)
}

/// The family a face of [`installed`] belongs to, found by the face's name.
pub fn family_of(face_name: &str) -> Option<&'static SystemFamily> {
    installed().iter().find(|f| f.faces.iter().any(|s| s.name == face_name))
}

fn font_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    if cfg!(windows) {
        let win = std::env::var_os("WINDIR").or_else(|| std::env::var_os("SystemRoot")).map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
        dirs.push(win.join("Fonts"));
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(PathBuf::from(local).join("Microsoft").join("Windows").join("Fonts"));
        }
    } else if cfg!(target_os = "macos") {
        dirs.extend(["/System/Library/Fonts", "/Library/Fonts"].map(PathBuf::from));
        dirs.extend(home.map(|h| h.join("Library").join("Fonts")));
    } else {
        dirs.extend(["/usr/share/fonts", "/usr/local/share/fonts"].map(PathBuf::from));
        if let Some(h) = home {
            dirs.push(h.join(".local").join("share").join("fonts"));
            dirs.push(h.join(".fonts"));
        }
    }
    dirs
}

fn scan() -> Vec<SystemFamily> {
    let mut files = Vec::new();
    for dir in font_dirs() {
        collect(&dir, 4, &mut files);
    }
    let mut families: Vec<SystemFamily> = Vec::new();
    for path in files {
        for (index, info) in read_faces(&path).into_iter().enumerate() {
            let Some(info) = info else { continue };
            let face = SystemFace { name: info.face_name(), path: path.clone(), index: index as u32, bold: info.bold, italic: info.italic };
            match families.iter_mut().find(|f| f.name == info.family) {
                Some(f) => {
                    f.thai |= info.thai;
                    if !f.faces.iter().any(|s| s.name == face.name) {
                        f.faces.push(face);
                    }
                }
                None => families.push(SystemFamily { name: info.family.clone(), thai: info.thai, faces: vec![face] }),
            }
        }
    }
    families.sort_by(|a, b| b.thai.cmp(&a.thai).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    families
}

/// Font files under `dir`, `depth` folders deep at most.
fn collect(dir: &Path, depth: u32, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let path = e.path();
        let Ok(kind) = e.file_type() else { continue };
        if kind.is_dir() {
            if depth > 0 {
                collect(&path, depth - 1, out);
            }
        } else if path.extension().and_then(|x| x.to_str()).is_some_and(|x| ["ttf", "ttc", "otf"].contains(&x.to_ascii_lowercase().as_str())) {
            out.push(path);
        }
    }
}

/// What the font list needs from one face.
#[derive(Clone, Debug, PartialEq)]
struct FaceInfo {
    family: String,
    style: String,
    bold: bool,
    italic: bool,
    thai: bool,
}

impl FaceInfo {
    fn face_name(&self) -> String {
        if self.style.is_empty() || self.style.eq_ignore_ascii_case("regular") {
            self.family.clone()
        } else {
            format!("{} {}", self.family, self.style)
        }
    }
}

/// Each face of a font file: `None` for faces that can't be embedded (no TrueType outlines, a
/// licence that forbids it, or unreadable).
fn read_faces(path: &Path) -> Vec<Option<FaceInfo>> {
    let Ok(mut f) = std::fs::File::open(path) else { return Vec::new() };
    let mut head = [0u8; 12];
    if f.read_exact(&mut head).is_err() {
        return Vec::new();
    }
    if &head[..4] == b"ttcf" {
        let faces = u32::from_be_bytes([head[8], head[9], head[10], head[11]]).min(64);
        let mut offsets = vec![0u8; faces as usize * 4];
        if f.read_exact(&mut offsets).is_err() {
            return Vec::new();
        }
        offsets.chunks(4).map(|o| face_info(&mut f, u64::from(u32::from_be_bytes([o[0], o[1], o[2], o[3]])))).collect()
    } else {
        vec![face_info(&mut f, 0)]
    }
}

fn read_at(f: &mut std::fs::File, at: u64, len: usize) -> Option<Vec<u8>> {
    if len > 4 << 20 {
        return None;
    }
    f.seek(SeekFrom::Start(at)).ok()?;
    let mut buf = vec![0u8; len];
    f.read_exact(&mut buf).ok()?;
    Some(buf)
}

fn face_info(f: &mut std::fs::File, offset: u64) -> Option<FaceInfo> {
    let header = read_at(f, offset, 12)?;
    let tables = usize::from(u16::from_be_bytes([header[4], header[5]]));
    if tables == 0 || tables > 512 {
        return None;
    }
    let dir = read_at(f, offset + 12, 16 * tables)?;
    let find = |tag: &[u8; 4]| {
        dir.chunks(16)
            .find(|r| &r[..4] == tag)
            .map(|r| (u64::from(u32::from_be_bytes([r[8], r[9], r[10], r[11]])), u32::from_be_bytes([r[12], r[13], r[14], r[15]]) as usize))
    };
    find(b"glyf")?;
    let (os2_at, os2_len) = find(b"OS/2")?;
    let os2 = read_at(f, os2_at, os2_len.min(96))?;
    let be16 = |b: &[u8], o: usize| b.get(o..o + 2).map(|x| u16::from_be_bytes([x[0], x[1]]));
    let fs_type = be16(&os2, 8)?;
    let licence = fs_type & 0x000f;
    if fs_type & 0x0200 != 0 || (licence & 0x0002 != 0 && licence & 0x000c == 0) {
        return None;
    }
    let weight = be16(&os2, 4).unwrap_or(400);
    let range1 = os2.get(42..46).map_or(0, |x| u32::from_be_bytes([x[0], x[1], x[2], x[3]]));
    let selection = be16(&os2, 62).unwrap_or(0);
    let (name_at, name_len) = find(b"name")?;
    let name = read_at(f, name_at, name_len)?;
    let family = name_string(&name, 16).or_else(|| name_string(&name, 1))?;
    let style = name_string(&name, 17).or_else(|| name_string(&name, 2)).unwrap_or_default();
    if family.starts_with('.') || family.trim().is_empty() {
        return None;
    }
    Some(FaceInfo {
        family: family.trim().to_owned(),
        style: style.trim().to_owned(),
        bold: selection & 0x20 != 0 || weight >= 600,
        italic: selection & 0x01 != 0,
        thai: range1 & (1 << 24) != 0,
    })
}

/// Name `id` from a `name` table: Windows Unicode English first, then any Windows Unicode, then
/// Macintosh Roman.
fn name_string(table: &[u8], id: u16) -> Option<String> {
    let be16 = |o: usize| table.get(o..o + 2).map(|x| u16::from_be_bytes([x[0], x[1]]));
    let count = usize::from(be16(2)?);
    let strings = usize::from(be16(4)?);
    let mut best: Option<(u8, String)> = None;
    for i in 0..count.min(1024) {
        let r = 6 + 12 * i;
        let (platform, encoding, language, name_id) = (be16(r)?, be16(r + 2)?, be16(r + 4)?, be16(r + 6)?);
        if name_id != id {
            continue;
        }
        let (len, off) = (usize::from(be16(r + 8)?), usize::from(be16(r + 10)?));
        let Some(raw) = table.get(strings + off..strings + off + len) else { continue };
        let (rank, text) = match (platform, encoding) {
            (3, 1 | 10) => {
                let units: Vec<u16> = raw.chunks(2).filter(|c| c.len() == 2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
                (if language == 0x0409 { 0 } else { 1 }, String::from_utf16_lossy(&units))
            }
            (1, 0) => (2, raw.iter().map(|&b| if b.is_ascii() { b as char } else { '?' }).collect()),
            _ => continue,
        };
        if best.as_ref().is_none_or(|(r, _)| rank < *r) {
            best = Some((rank, text));
        }
    }
    best.map(|(_, t)| t).filter(|t| !t.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_font(name: &str, bytes: &[u8]) -> PathBuf {
        let path = std::env::temp_dir().join(format!("pdfkub-font-list-{}-{name}", std::process::id()));
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn the_bundled_thai_faces_are_read_from_their_headers() {
        let path = temp_font("Anuphan-SemiBold.ttf", pdfcraft_fonts::ANUPHAN_SEMIBOLD);
        let faces = read_faces(&path);
        let info = faces[0].as_ref().expect("an embeddable TrueType face");
        assert_eq!(info.family, "Anuphan");
        assert!(info.bold && !info.italic && info.thai, "{info:?}");
        assert_eq!(info.face_name(), format!("Anuphan {}", info.style));
        let face = SystemFace { name: info.face_name(), path: path.clone(), index: 0, bold: true, italic: false };
        assert!(face.load().unwrap().covers("ภาษาไทย"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn styles_fall_back_to_the_nearest_face() {
        let face = |name: &str, bold, italic| SystemFace { name: name.into(), path: PathBuf::new(), index: 0, bold, italic };
        let fam = SystemFamily { name: "X".into(), thai: false, faces: vec![face("X", false, false), face("X Bold", true, false)] };
        assert_eq!(fam.face(true, true).unwrap().name, "X Bold");
        assert_eq!(fam.face(false, true).unwrap().name, "X");
    }
}
