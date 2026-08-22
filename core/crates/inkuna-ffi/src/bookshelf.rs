//! The exported library object and the plumbing every method shares.

use std::sync::{Arc, Mutex, OnceLock, Weak};

use crate::error::InkunaError;
use crate::import::ShelfImport;
use crate::library::ShelfLibrary;
use crate::progress::ShelfProgress;
use crate::reader::{
    LayoutListener, ListenerAdapter, ReaderLayoutSettings, ReaderSession, Viewport,
};
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
    pub(crate) library: Arc<inkuna_core::Library>,
    pub(crate) font_dir: std::path::PathBuf,
    /// The bundled font set, loaded once per process on first reader
    /// open (off the UI thread) and shared by every later session.
    /// `Arc`-wrapped so `open_reader` can move a handle onto the
    /// blocking pool.
    font_registry: Arc<OnceLock<Arc<inkuna_core::FontRegistry>>>,
    /// Last-open-wins: the one live engine session per `Bookshelf`,
    /// held weakly so a shell dropping its `ReaderSession` closes the
    /// engine session without this registry keeping it alive.
    active_session: Arc<Mutex<Weak<inkuna_core::EngineSession>>>,
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
            font_registry: Arc::new(OnceLock::new()),
            active_session: Arc::new(Mutex::new(Weak::new())),
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

#[uniffi::export(async_runtime = "tokio")]
impl Bookshelf {
    /// Opens the reader engine on one publication: resolves the book,
    /// loads the bundled fonts (once per process), and starts the layout
    /// worker at the stored coordinate's chapter (chapter 0 when none).
    ///
    /// Last-open-wins: one live reader per `Bookshelf` — a still-live
    /// previous session (any id) is closed before the new one opens;
    /// sessions also close when the shell drops them. `listener`
    /// callbacks arrive on engine threads — hop to the main thread.
    ///
    /// Fixed-layout books throw `UnsupportedContent`; an unknown id
    /// `NotFound`; a broken font dir `UnsupportedContent`.
    pub async fn open_reader(
        &self,
        id: String,
        viewport: Viewport,
        settings: ReaderLayoutSettings,
        listener: Arc<dyn LayoutListener>,
    ) -> Result<Arc<ReaderSession>, InkunaError> {
        let library = self.library.clone();
        let font_dir = self.font_dir.clone();
        let registry = self.font_registry.clone();
        let active = self.active_session.clone();
        blocking(move || {
            let publication = library.publication(&id)?;
            let epub_path = library.data_dir().join(&publication.file_path);
            let opening_chapter = publication
                .coordinate
                .as_ref()
                .map(|c| c.spine_idx)
                .unwrap_or(0);

            // Loaded once per process; every face parses eagerly, so a
            // bad bundle fails here rather than mid-shaping.
            let fonts = match registry.get() {
                Some(fonts) => fonts.clone(),
                None => {
                    let loaded = inkuna_core::FontRegistry::load(&font_dir).map_err(|e| {
                        InkunaError::UnsupportedContent {
                            detail: format!("font registry: {e}"),
                        }
                    })?;
                    // A concurrent first open may have won the race; both
                    // loaded the same fixed set, so either value is right.
                    let _ = registry.set(loaded.clone());
                    registry.get().cloned().unwrap_or(loaded)
                }
            };

            // The synthetic-position snapshot the session answers
            // `position_of`/`position_count` from without touching the DB.
            let ranges = library.position_ranges(&id)?;

            // Last-open-wins, before the new open so two workers never
            // lay out concurrently.
            let mut slot = active.lock().unwrap();
            if let Some(previous) = slot.upgrade() {
                previous.close();
            }
            let session = inkuna_core::EngineSession::open(
                &epub_path,
                fonts.clone(),
                viewport.into(),
                settings.into(),
                publication.language.clone(),
                opening_chapter,
                Arc::new(ListenerAdapter(listener)),
            )
            .map_err(|e| InkunaError::from(inkuna_core::CoreError::from(e)))?;
            *slot = Arc::downgrade(&session);
            drop(slot);

            Ok(Arc::new(ReaderSession {
                session,
                fonts,
                ranges,
            }))
        })
        .await
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
