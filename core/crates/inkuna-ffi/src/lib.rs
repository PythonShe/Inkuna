//! FFI surface for the Inkuna core, exported via UniFFI to Swift and Kotlin.
//!
//! Types here mirror `inkuna-core` deliberately: the core stays pure Rust
//! (free to use references, borrows, rich enums) while this layer keeps the
//! boundary coarse-grained and owned-value only.

use std::sync::Arc;

uniffi::setup_scaffolding!("inkuna");

#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum InkunaError {
    #[error("io error: {0}")]
    Io(String),
    #[error("database error: {0}")]
    Database(String),
    #[error("archive error: {0}")]
    Archive(String),
    #[error("unsupported format")]
    UnsupportedFormat,
    #[error("invalid publication: {0}")]
    InvalidPublication(String),
    #[error("publication not found: {0}")]
    NotFound(String),
}

impl From<inkuna_core::CoreError> for InkunaError {
    fn from(e: inkuna_core::CoreError) -> Self {
        use inkuna_core::CoreError as C;
        match e {
            C::Io(e) => InkunaError::Io(e.to_string()),
            C::Database(e) => InkunaError::Database(e.to_string()),
            C::Archive(m) => InkunaError::Archive(m),
            C::UnsupportedFormat => InkunaError::UnsupportedFormat,
            C::InvalidPublication(m) => InkunaError::InvalidPublication(m),
            C::NotFound(id) => InkunaError::NotFound(id),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Format {
    Epub,
    Cbz,
    Cbr,
}

impl From<inkuna_core::Format> for Format {
    fn from(f: inkuna_core::Format) -> Self {
        match f {
            inkuna_core::Format::Epub => Format::Epub,
            inkuna_core::Format::Cbz => Format::Cbz,
            inkuna_core::Format::Cbr => Format::Cbr,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct Publication {
    pub id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub language: Option<String>,
    pub format: Format,
    pub file_path: String,
    pub added_at: i64,
    pub progression: f64,
}

impl From<inkuna_core::Publication> for Publication {
    fn from(p: inkuna_core::Publication) -> Self {
        Publication {
            id: p.id,
            title: p.title,
            authors: p.authors,
            language: p.language,
            format: p.format.into(),
            file_path: p.file_path,
            added_at: p.added_at,
            progression: p.progression,
        }
    }
}

#[derive(uniffi::Object)]
pub struct Bookshelf(inkuna_core::Library);

#[uniffi::export]
impl Bookshelf {
    #[uniffi::constructor]
    pub fn open(db_path: String) -> Result<Arc<Self>, InkunaError> {
        Ok(Arc::new(Bookshelf(inkuna_core::Library::open(&db_path)?)))
    }

    pub fn import(&self, path: String) -> Result<Publication, InkunaError> {
        Ok(self.0.import(&path)?.into())
    }

    pub fn list(&self) -> Result<Vec<Publication>, InkunaError> {
        Ok(self.0.list()?.into_iter().map(Into::into).collect())
    }

    pub fn set_progression(&self, id: String, progression: f64) -> Result<(), InkunaError> {
        Ok(self.0.set_progression(&id, progression)?)
    }

    pub fn remove(&self, id: String) -> Result<(), InkunaError> {
        Ok(self.0.remove(&id)?)
    }
}

#[uniffi::export]
pub fn core_version() -> String {
    inkuna_core::version().to_string()
}
