//! The container layer's error type. Variants mirror the `CoreError`
//! distinctions the shells route on — an unreadable file (`Io`), a damaged
//! container (`Archive`), and a broken book (`InvalidPublication`) stay
//! apart — and `inkuna-core` maps them back one-for-one, so nothing is
//! reshaped at the crate boundary.

/// Every fallible operation in this crate fails with one of these.
#[derive(Debug, thiserror::Error)]
pub enum ContentError {
    /// The filesystem failed: the archive is unreadable, storage errored
    /// mid-read. Never a statement about the content of a book.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// The zip container is damaged or uses something unsupported (a
    /// compression method, a corrupt central directory) — as distinct
    /// from a well-formed archive whose EPUB structure is wrong, which is
    /// `InvalidPublication`.
    #[error("archive error: {0}")]
    Archive(String),

    /// The file is the format it claims to be but a mandatory part is
    /// missing or malformed, or an entry blows a decompression cap. The
    /// message names the specific failure for logs; it is not a
    /// user-facing string.
    #[error("invalid publication: {0}")]
    InvalidPublication(String),

    /// The source is bigger than a hard byte ceiling; carries the ceiling
    /// in bytes so a caller can name the limit it just hit.
    #[error("file too large: over {0} bytes")]
    FileTooLarge(u64),
}

impl From<zip::result::ZipError> for ContentError {
    fn from(e: zip::result::ZipError) -> Self {
        ContentError::Archive(e.to_string())
    }
}
