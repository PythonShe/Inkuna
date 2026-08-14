use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::CoreError;

/// Supported publication formats. Detection is by content (magic bytes),
/// never by file extension, so misnamed files import correctly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Epub,
    Cbz,
    Cbr,
}

const ZIP_MAGIC: &[u8] = b"PK\x03\x04";
// Shared prefix of the RAR4 (…\x07\x00) and RAR5 (…\x07\x01\x00) signatures.
const RAR_MAGIC: &[u8] = b"Rar!\x1a\x07";

impl Format {
    pub fn detect(path: &Path) -> Result<Format, CoreError> {
        let mut magic = [0u8; 8];
        let n = File::open(path)?.read(&mut magic)?;

        if n >= RAR_MAGIC.len() && magic.starts_with(RAR_MAGIC) {
            return Ok(Format::Cbr);
        }
        if n >= ZIP_MAGIC.len() && magic.starts_with(ZIP_MAGIC) {
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
        Err(CoreError::UnsupportedFormat)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Format::Epub => "epub",
            Format::Cbz => "cbz",
            Format::Cbr => "cbr",
        }
    }

    pub fn from_str(s: &str) -> Result<Format, CoreError> {
        match s {
            "epub" => Ok(Format::Epub),
            "cbz" => Ok(Format::Cbz),
            "cbr" => Ok(Format::Cbr),
            _ => Err(CoreError::UnsupportedFormat),
        }
    }
}
