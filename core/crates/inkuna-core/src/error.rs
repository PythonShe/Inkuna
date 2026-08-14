#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("archive error: {0}")]
    Archive(String),

    #[error("unsupported format")]
    UnsupportedFormat,

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
