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
    #[error("unsupported format{}", .0.as_deref().map(|f| format!(": {f}")).unwrap_or_default())]
    UnsupportedFormat(Option<String>),
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
            C::UnsupportedFormat(f) => InkunaError::UnsupportedFormat(f),
            C::InvalidPublication(m) => InkunaError::InvalidPublication(m),
            C::NotFound(id) => InkunaError::NotFound(id),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Format {
    Epub,
    Mobi,
    Azw3,
    Txt,
    Pdf,
    Cbz,
    Cbr,
}

impl From<inkuna_core::Format> for Format {
    fn from(f: inkuna_core::Format) -> Self {
        match f {
            inkuna_core::Format::Epub => Format::Epub,
            inkuna_core::Format::Mobi => Format::Mobi,
            inkuna_core::Format::Azw3 => Format::Azw3,
            inkuna_core::Format::Txt => Format::Txt,
            inkuna_core::Format::Pdf => Format::Pdf,
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
pub struct Bookshelf(Arc<inkuna_core::Library>);

/// Methods are async on a tokio runtime: SQLite and archive I/O run on
/// blocking threads while the shells get idiomatic Swift `await` / Kotlin
/// `suspend` — never a blocked main thread.
#[uniffi::export(async_runtime = "tokio")]
impl Bookshelf {
    /// `data_dir` is the core-owned storage root (Application Support /
    /// `filesDir`): the DB, imported books, and covers all live under it.
    #[uniffi::constructor]
    pub fn open(data_dir: String) -> Result<Arc<Self>, InkunaError> {
        Ok(Arc::new(Bookshelf(Arc::new(inkuna_core::Library::open(&data_dir)?))))
    }

    pub async fn import(&self, path: String) -> Result<Publication, InkunaError> {
        let library = self.0.clone();
        blocking(move || {
            let publication = match library.import(&path)? {
                inkuna_core::ImportOutcome::Imported(p)
                | inkuna_core::ImportOutcome::Duplicate(p) => p,
            };
            Ok(publication.into())
        })
        .await
    }

    pub async fn list(&self) -> Result<Vec<Publication>, InkunaError> {
        let library = self.0.clone();
        blocking(move || Ok(library.list()?.into_iter().map(Into::into).collect())).await
    }

    /// One call per page turn. `locator` is the opaque Readium locator
    /// JSON; `progression` is the book-wide totalProgression;
    /// `position` the synthetic position, once the navigator knows it.
    pub async fn update_progress(
        &self,
        id: String,
        locator: String,
        progression: f64,
        position: Option<u32>,
    ) -> Result<(), InkunaError> {
        let library = self.0.clone();
        blocking(move || Ok(library.update_progress(&id, &locator, progression, position)?)).await
    }

    pub async fn remove(&self, id: String) -> Result<(), InkunaError> {
        let library = self.0.clone();
        blocking(move || Ok(library.remove(&id)?)).await
    }
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, InkunaError> + Send + 'static,
) -> Result<T, InkunaError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| InkunaError::Io(format!("task join error: {e}")))?
}

#[uniffi::export]
pub fn core_version() -> String {
    inkuna_core::version().to_string()
}
