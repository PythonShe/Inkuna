use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::str::FromStr;

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
/// Decompression budget for the `mimetype` entry. The EPUB spec fixes its
/// content to the literal `application/epub+zip` (20 bytes), so 256 leaves
/// room for stray whitespace or a BOM and nothing else: the entry may be
/// Deflated, and without a cap a crafted one inflates to gigabytes here —
/// before any of the capped readers in `formats::epub::archive` is reached.
const MAX_MIMETYPE_BYTES: u64 = 256;

impl Format {
    /// Detects the format of the file at `path` from its content, never
    /// its extension — the one exception being TXT, which has no magic
    /// bytes and needs `.txt` plus a NUL-free sample. A ZIP is an EPUB only
    /// if its `mimetype` entry says so (read under a 256-byte cap, so a
    /// crafted entry cannot inflate here) and any other ZIP is taken for a
    /// CBZ. Returns `UnsupportedFormat(None)` when nothing matches, and
    /// `Io` when the file cannot be read at all.
    pub fn detect(path: &Path) -> Result<Format, CoreError> {
        Self::detect_as(path, path)
    }

    /// [`detect`](Self::detect) with the filename decoupled from the
    /// content: reads the bytes at `content` while `named` supplies the
    /// extension for the TXT check. For staged copies of a stream import,
    /// whose real name travels separately from the bytes.
    pub fn detect_as(content: &Path, named: &Path) -> Result<Format, CoreError> {
        let path = content;
        let mut file = File::open(path)?;
        let mut head = [0u8; 4 * 1024];
        // `read` may return short: fill the sample until EOF or the buffer
        // is full, so the magic checks below never see a truncated head.
        let mut n = 0;
        while n < head.len() {
            match file.read(&mut head[n..]) {
                Ok(0) => break,
                Ok(read) => n += read,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.into()),
            }
        }

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
            if let Ok(entry) = archive.by_name("mimetype") {
                let mut mime = String::new();
                // One byte past the budget, so an entry that fills the cap
                // exactly is distinguishable from one that overflows it.
                let _ = entry.take(MAX_MIMETYPE_BYTES + 1).read_to_string(&mut mime);
                if mime.len() as u64 <= MAX_MIMETYPE_BYTES && mime.trim() == "application/epub+zip"
                {
                    return Ok(Format::Epub);
                }
            }
            return Ok(Format::Cbz);
        }
        if is_plain_text(named, &head[..n]) {
            return Ok(Format::Txt);
        }
        Err(CoreError::UnsupportedFormat(None))
    }

    /// The lowercase tag persisted in the `publications.format` column.
    /// It is a stored value, so these strings are part of the on-disk
    /// schema: rename one and every existing row stops mapping.
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

    /// Inverse of [`as_str`](Self::as_str), used when mapping a row back
    /// out of the DB. An unrecognized tag — a row written by a newer core
    /// that knows a format this one does not — is
    /// `UnsupportedFormat(Some(tag))`, never a silent default.
    ///
    /// Kept as an inherent method so existing `Format::from_str(s)` call
    /// sites need no `FromStr` import; it delegates to the trait impl.
    // `should_implement_trait` fires on any inherent `from_str`, even when
    // `FromStr` is implemented (it is, below); removing this shim would drop
    // a public method the crate's consumers already call.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Result<Format, CoreError> {
        <Format as FromStr>::from_str(s)
    }
}

impl FromStr for Format {
    type Err = CoreError;

    /// Parses the lowercase tag written by [`as_str`](Format::as_str).
    /// See [`Format::from_str`] for the unrecognized-tag contract.
    fn from_str(s: &str) -> Result<Format, CoreError> {
        match s {
            "epub" => Ok(Format::Epub),
            "mobi" => Ok(Format::Mobi),
            "azw3" => Ok(Format::Azw3),
            "txt" => Ok(Format::Txt),
            "pdf" => Ok(Format::Pdf),
            "cbz" => Ok(Format::Cbz),
            "cbr" => Ok(Format::Cbr),
            _ => Err(CoreError::UnsupportedFormat(Some(s.to_string()))),
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

/// TXT has no magic bytes: require the `.txt` extension and reject NUL-heavy
/// binary data, while admitting UTF-16 BOMs and the same NUL-density
/// BOM-less UTF-16 sample recognized by the converter.
fn is_plain_text(path: &Path, sample: &[u8]) -> bool {
    let is_txt_ext = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("txt"));
    if !is_txt_ext {
        return false;
    }
    if sample.starts_with(&[0xff, 0xfe]) || sample.starts_with(&[0xfe, 0xff]) {
        return true;
    }
    let nulls = sample.iter().filter(|byte| **byte == 0).count();
    if nulls == 0 {
        return true;
    }
    // NUL-bearing content is admitted only when the converter's shared
    // heuristic would decode it as BOM-less UTF-16, so detection and
    // decoding always agree.
    super::txt::bomless_utf16(sample).is_some()
}

#[cfg(test)]
#[path = "format_tests.rs"]
mod tests;
