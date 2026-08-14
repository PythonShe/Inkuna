use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::CoreError;

/// Supported publication formats. Detection is by content (magic bytes) —
/// the single exception is TXT, which has no magic and is admitted by
/// `.txt` extension plus a binary-content sanity check.
///
/// Reflowable formats (EPUB, MOBI, AZW3, TXT) normalize to EPUB at import;
/// fixed-layout formats (PDF, comics) render via dedicated navigators.
/// MOBI/AZW3 support covers DRM-free files only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Epub,
    Mobi,
    Azw3,
    Txt,
    Pdf,
    Cbz,
    Cbr,
}

const ZIP_MAGIC: &[u8] = b"PK\x03\x04";
// Shared prefix of the RAR4 (…\x07\x00) and RAR5 (…\x07\x01\x00) signatures.
const RAR_MAGIC: &[u8] = b"Rar!\x1a\x07";
const PDF_MAGIC: &[u8] = b"%PDF-";
// PalmDB type+creator fields at offset 60 for Mobipocket-family files.
const PALMDB_BOOKMOBI: &[u8] = b"BOOKMOBI";

impl Format {
    pub fn detect(path: &Path) -> Result<Format, CoreError> {
        let mut file = File::open(path)?;
        let mut head = [0u8; 68];
        let n = file.read(&mut head)?;

        if n >= RAR_MAGIC.len() && head.starts_with(RAR_MAGIC) {
            return Ok(Format::Cbr);
        }
        if n >= PDF_MAGIC.len() && head.starts_with(PDF_MAGIC) {
            return Ok(Format::Pdf);
        }
        if n >= 68 && &head[60..68] == PALMDB_BOOKMOBI {
            return Ok(detect_mobi_generation(&mut file));
        }
        if n >= ZIP_MAGIC.len() && head.starts_with(ZIP_MAGIC) {
            // A ZIP container is an EPUB iff its `mimetype` entry says so;
            // any other archive of images is treated as CBZ.
            let mut archive = zip::ZipArchive::new(File::open(path)?)?;
            if let Ok(mut entry) = archive.by_name("mimetype") {
                let mut mime = String::new();
                let _ = entry.read_to_string(&mut mime);
                if mime.trim() == "application/epub+zip" {
                    return Ok(Format::Epub);
                }
            }
            return Ok(Format::Cbz);
        }
        if is_plain_text(path, &head[..n]) {
            return Ok(Format::Txt);
        }
        Err(CoreError::UnsupportedFormat)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Format::Epub => "epub",
            Format::Mobi => "mobi",
            Format::Azw3 => "azw3",
            Format::Txt => "txt",
            Format::Pdf => "pdf",
            Format::Cbz => "cbz",
            Format::Cbr => "cbr",
        }
    }

    pub fn from_str(s: &str) -> Result<Format, CoreError> {
        match s {
            "epub" => Ok(Format::Epub),
            "mobi" => Ok(Format::Mobi),
            "azw3" => Ok(Format::Azw3),
            "txt" => Ok(Format::Txt),
            "pdf" => Ok(Format::Pdf),
            "cbz" => Ok(Format::Cbz),
            "cbr" => Ok(Format::Cbr),
            _ => Err(CoreError::UnsupportedFormat),
        }
    }
}

/// Distinguishes classic MOBI from AZW3 (KF8) by the file version in the
/// MOBI header of PalmDB record 0. Layout: record entries start at byte 78,
/// 8 bytes each with the record offset first; record 0 holds a 16-byte
/// PalmDoc header, then the "MOBI" header whose file version sits at +36.
/// Malformed structures fall back to Mobi — the importer will surface a
/// real parse error later.
fn detect_mobi_generation(file: &mut File) -> Format {
    fn version(file: &mut File) -> Option<u32> {
        let mut entry = [0u8; 8];
        file.seek(SeekFrom::Start(78)).ok()?;
        file.read_exact(&mut entry).ok()?;
        let record0 = u32::from_be_bytes(entry[0..4].try_into().unwrap()) as u64;

        let mut header = [0u8; 40];
        file.seek(SeekFrom::Start(record0)).ok()?;
        file.read_exact(&mut header).ok()?;
        if &header[16..20] != b"MOBI" {
            return None;
        }
        Some(u32::from_be_bytes(header[36..40].try_into().unwrap()))
    }
    match version(file) {
        Some(v) if v >= 8 => Format::Azw3,
        _ => Format::Mobi,
    }
}

/// TXT has no magic bytes: require the `.txt` extension AND a sample free
/// of NUL bytes, so binaries renamed to .txt are rejected while non-UTF-8
/// CJK encodings (GB18030, Big5, Shift-JIS) still pass.
fn is_plain_text(path: &Path, sample: &[u8]) -> bool {
    let is_txt_ext = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("txt"));
    is_txt_ext && !sample.contains(&0)
}
