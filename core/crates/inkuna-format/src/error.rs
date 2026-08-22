//! The conversion layer's error type. Variants mirror the `CoreError`
//! distinctions the moved converters construct, and `inkuna-core` maps
//! them back one-for-one, so nothing is reshaped at the crate boundary.

/// Every fallible operation in this crate fails with one of these.
#[derive(Debug, thiserror::Error)]
pub enum FormatError {
    /// The filesystem failed: the source is unreadable, storage is full,
    /// a device error hit mid-write. Never a statement about the content
    /// of a book.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// The zip container being written or read is damaged or uses
    /// something unsupported.
    #[error("archive error: {0}")]
    Archive(String),

    /// The file is the format it claims to be but cannot be converted: a
    /// mandatory structure is missing, a bound tripped, the content is
    /// encrypted. The message names the specific failure for logs; it is
    /// not a user-facing string.
    #[error("invalid publication: {0}")]
    InvalidPublication(String),

    /// The format was recognized but this build cannot convert it;
    /// carries the detected format name when one is known.
    #[error("unsupported format{}", .0.as_deref().map(|f| format!(": {f}")).unwrap_or_default())]
    UnsupportedFormat(Option<String>),

    /// The source is bigger than a hard byte ceiling; carries the ceiling
    /// in bytes so a caller can name the limit it just hit.
    #[error("file too large: over {0} bytes")]
    FileTooLarge(u64),
}

impl From<zip::result::ZipError> for FormatError {
    fn from(e: zip::result::ZipError) -> Self {
        FormatError::Archive(e.to_string())
    }
}
