#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("archive error: {0}")]
    Archive(String),

    /// Carries the detected format name when one was recognized but is not
    /// yet importable ("mobi"), so shells can say "MOBI support is coming"
    /// instead of a bare error; `None` means nothing recognizable at all.
    #[error("unsupported format{}", .0.as_deref().map(|f| format!(": {f}")).unwrap_or_default())]
    UnsupportedFormat(Option<String>),

    #[error("invalid publication: {0}")]
    InvalidPublication(String),

    #[error("publication not found: {0}")]
    NotFound(String),
}

impl From<zip::result::ZipError> for CoreError {
    fn from(e: zip::result::ZipError) -> Self {
        CoreError::Archive(e.to_string())
    }
}
