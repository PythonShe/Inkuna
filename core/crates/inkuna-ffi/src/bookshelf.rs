//! The exported library object and the plumbing every method shares.

use std::sync::Arc;

use crate::error::InkunaError;
use crate::import::ShelfImport;
use crate::library::ShelfLibrary;
use crate::progress::ShelfProgress;
use crate::search::ShelfSearch;
use crate::settings::ShelfSettings;
use crate::stats::ShelfStats;

/// The one root UniFFI object; named `Bookshelf` because UniFFI's Kotlin
/// output imports JNA's `com.sun.jna.Library` and `Library` would
/// collide. Feature methods live on per-feature facade objects
/// (`ShelfLibrary`, `ShelfImport`, …), each wrapping the same shared
/// core library and constructed once here; the accessors below hand
/// them out as cheap `Arc` clones, no I/O.
///
/// `font_dir` is the bundled fonts directory the reader engine shapes
/// with; the shells pass their bundled copy of repo `assets/fonts/`.
///
/// Facade methods are async on a tokio runtime: SQLite and archive I/O
/// run on blocking threads while the shells get idiomatic Swift `await`
/// / Kotlin `suspend` — never a blocked main thread.
#[derive(uniffi::Object)]
pub struct Bookshelf {
    // Both fields are consumed by the reader-session facade the engine
    // swap lands next; the shape is final now so it only adds call sites.
    #[allow(dead_code)]
    pub(crate) library: Arc<inkuna_core::Library>,
    #[allow(dead_code)]
    pub(crate) font_dir: std::path::PathBuf,
    library_facade: Arc<ShelfLibrary>,
    importer: Arc<ShelfImport>,
    search: Arc<ShelfSearch>,
    settings: Arc<ShelfSettings>,
    progress: Arc<ShelfProgress>,
    stats: Arc<ShelfStats>,
}

#[uniffi::export]
impl Bookshelf {
    /// `data_dir` is the core-owned storage root (Application Support /
    /// `filesDir`): the DB, imported books, and covers all live under it.
    /// `font_dir` must be an existing directory holding the bundled
    /// reader fonts; a missing one fails here, at startup, rather than at
    /// first reader open.
    ///
    /// Hold exactly one `Bookshelf` per `data_dir` for the process lifetime:
    /// opening sweeps files no row references, so a second concurrent
    /// instance on the same directory deletes the first one's in-flight
    /// import.
    #[uniffi::constructor]
    pub fn open(data_dir: String, font_dir: String) -> Result<Arc<Self>, InkunaError> {
        let font_dir = std::path::PathBuf::from(font_dir);
        if !font_dir.is_dir() {
            return Err(InkunaError::Io {
                detail: format!("font_dir does not exist: {}", font_dir.display()),
            });
        }
        let library = Arc::new(inkuna_core::Library::open(&data_dir)?);
        Ok(Arc::new(Bookshelf {
            library_facade: Arc::new(ShelfLibrary(library.clone())),
            importer: Arc::new(ShelfImport(library.clone())),
            search: Arc::new(ShelfSearch(library.clone())),
            settings: Arc::new(ShelfSettings(library.clone())),
            progress: Arc::new(ShelfProgress(library.clone())),
            stats: Arc::new(ShelfStats(library.clone())),
            library,
            font_dir,
        }))
    }

    pub fn library(&self) -> Arc<ShelfLibrary> {
        self.library_facade.clone()
    }

    pub fn importer(&self) -> Arc<ShelfImport> {
        self.importer.clone()
    }

    pub fn search(&self) -> Arc<ShelfSearch> {
        self.search.clone()
    }

    pub fn settings(&self) -> Arc<ShelfSettings> {
        self.settings.clone()
    }

    pub fn progress(&self) -> Arc<ShelfProgress> {
        self.progress.clone()
    }

    pub fn stats(&self) -> Arc<ShelfStats> {
        self.stats.clone()
    }
}

/// Runs sync core work on tokio's blocking pool, the bridge every async
/// method goes through.
pub(crate) async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, InkunaError> + Send + 'static,
) -> Result<T, InkunaError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| InkunaError::Io {
            detail: format!("task join error: {e}"),
        })?
}

#[uniffi::export]
pub fn core_version() -> String {
    inkuna_core::version().to_string()
}
