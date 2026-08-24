//! The exported library object and the plumbing every method shares.

use std::sync::{Arc, Mutex, OnceLock, Weak};

/// Request-ordered last-open-wins slot for the one live engine session.
///
/// Concurrent `open_reader` calls complete in arbitrary order on the
/// blocking pool, so completion order cannot decide the winner: a slow
/// older open finishing last must not displace the newer, currently
/// visible session. Every request draws a monotonically increasing
/// ticket up front; only the holder of the newest ticket may install
/// its session, and a stale request instead gets its own session back
/// to close off to the side.
///
/// Both methods only swap under the lock — closing sessions happens in
/// the caller, outside any critical section — and both recover from
/// poisoning: the state is swap-consistent, so a panicking open never
/// bricks later ones. Generic over the session type purely so the
/// ordering rules are unit-testable without opening real books.
struct ActiveSlot<S> {
    state: Mutex<SlotState<S>>,
}

struct SlotState<S> {
    /// Tickets issued so far; the highest is the newest open request.
    issued: u64,
    /// The installed session, held weakly so a shell dropping its
    /// handle closes the engine session without this slot keeping it
    /// alive.
    session: Weak<S>,
}

impl<S> ActiveSlot<S> {
    fn new() -> Self {
        ActiveSlot {
            state: Mutex::new(SlotState {
                issued: 0,
                session: Weak::new(),
            }),
        }
    }

    /// Registers a new open request: draws its ticket and vacates the
    /// slot, returning the previous session (when still alive) for the
    /// caller to close outside the lock.
    fn begin(&self) -> (u64, Option<Arc<S>>) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.issued += 1;
        let previous = std::mem::replace(&mut state.session, Weak::new()).upgrade();
        (state.issued, previous)
    }

    /// Installs `session` only while `ticket` is still the newest
    /// issued. Returns the session the caller must close outside the
    /// lock: the displaced one on install, or `session` itself when a
    /// newer request was issued meanwhile — the newer session (whether
    /// already stored or still opening) keeps the slot.
    fn store(&self, ticket: u64, session: &Arc<S>) -> Option<Arc<S>> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if ticket == state.issued {
            std::mem::replace(&mut state.session, Arc::downgrade(session)).upgrade()
        } else {
            Some(session.clone())
        }
    }
}

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
    /// engine session without this registry keeping it alive. Ordered
    /// by request ticket, never by completion — see [`ActiveSlot`].
    active_session: Arc<ActiveSlot<inkuna_core::EngineSession>>,
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
            active_session: Arc::new(ActiveSlot::new()),
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

    /// The library facade — publication rows, the flattened TOC, and
    /// bookmarks. An `Arc` clone of the instance built in
    /// [`Bookshelf::open`]: no I/O and no second connection, so call it
    /// per screen rather than caching it. Every facade shares this
    /// `Bookshelf`'s one database, is safe to use from any thread, and
    /// keeps that database alive on its own — a facade outliving the
    /// `Bookshelf` it came from still works.
    pub fn library(&self) -> Arc<ShelfLibrary> {
        self.library_facade.clone()
    }

    /// The import facade — file and file-descriptor imports plus cover
    /// re-optimization. Same cheap `Arc` clone as [`Bookshelf::library`];
    /// the copying and conversion work happens inside the awaited
    /// methods, never here.
    pub fn importer(&self) -> Arc<ShelfImport> {
        self.importer.clone()
    }

    /// The search facade — the exact in-book scan and the tantivy-backed
    /// library-wide ranked search. Cheap `Arc` clone: the index was
    /// opened and reconciled against the DB back in [`Bookshelf::open`],
    /// so this accessor itself never touches disk.
    pub fn search(&self) -> Arc<ShelfSearch> {
        self.search.clone()
    }

    /// The settings facade — the reader's persisted preferences (theme,
    /// reminder, the Customize values). Cheap `Arc` clone; nothing is
    /// read or clamped until one of its methods is awaited.
    pub fn settings(&self) -> Arc<ShelfSettings> {
        self.settings.clone()
    }

    /// The progress facade — per-page-turn position writes plus the
    /// session-free synthetic-position queries that let Home and Detail
    /// label "page N of M" without opening a reader. Cheap `Arc` clone,
    /// no I/O.
    pub fn progress(&self) -> Arc<ShelfProgress> {
        self.progress.clone()
    }

    /// The stats facade — reading-session start/end and the overview
    /// aggregated from those sessions. Cheap `Arc` clone, no I/O.
    pub fn stats(&self) -> Arc<ShelfStats> {
        self.stats.clone()
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl Bookshelf {
    /// Registers the platform's system faces with the reader engine —
    /// they serve the roster's "system-serif"/"system-sans" reading
    /// fonts; the fallback stages stay the bundled Notos.
    ///
    /// Call AT MOST ONCE per process, before the first `open_reader`:
    /// registration loads the whole font registry (bundled set + system
    /// faces, off the UI thread), so the ids the shells prime from
    /// `font_registry()` are stable for the process lifetime. Once the
    /// registry exists — a reader was opened first, or a previous
    /// registration succeeded — this throws `InvalidState`.
    ///
    /// Degrades gracefully: each face that fails to load is skipped and
    /// reported in the returned warnings (log them); a role left with
    /// no usable upright face silently falls back to the bundled Noto
    /// equivalent at selection time. Only a broken BUNDLED set errors.
    pub async fn register_system_fonts(
        &self,
        faces: Vec<crate::fonts::SystemFontFace>,
    ) -> Result<Vec<crate::fonts::SystemFontWarning>, InkunaError> {
        let font_dir = self.font_dir.clone();
        let registry = self.font_registry.clone();
        blocking(move || {
            if registry.get().is_some() {
                return Err(InkunaError::InvalidState {
                    detail: "font registry already loaded; register system fonts \
                             before the first reader open"
                        .to_string(),
                });
            }
            let faces: Vec<inkuna_core::SystemFontFace> =
                faces.into_iter().map(Into::into).collect();
            let (loaded, warnings) =
                inkuna_core::FontRegistry::load_with_system(&font_dir, &faces).map_err(
                    |e| InkunaError::UnsupportedContent {
                        detail: format!("font registry: {e}"),
                    },
                )?;
            if registry.set(loaded).is_err() {
                // A concurrent first reader open won the race and its
                // registry (without these faces) is already primed.
                return Err(InkunaError::InvalidState {
                    detail: "font registry already loaded; register system fonts \
                             before the first reader open"
                        .to_string(),
                });
            }
            Ok(warnings.into_iter().map(Into::into).collect())
        })
        .await
    }

    /// Opens the reader engine on one publication: resolves the book,
    /// loads the bundled fonts (once per process), and starts the layout
    /// worker at the stored coordinate's chapter (chapter 0 when none).
    ///
    /// Last-open-wins, ordered by REQUEST: one live reader per
    /// `Bookshelf` — a still-live previous session (any id) is closed
    /// before the new one opens, and a concurrent open that was
    /// requested later always wins even when it completes first (the
    /// earlier call then returns an already-closed session); sessions
    /// also close when the shell drops them. `listener` callbacks
    /// arrive on engine threads — hop to the main thread.
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
        // The ticket is drawn HERE, before hopping to the blocking pool,
        // so concurrent opens are ordered by request, not by whichever
        // blocking task happens to finish last — a slow older open can
        // never displace the newer session the shell is showing.
        let (ticket, previous) = active.begin();
        blocking(move || {
            if let Some(previous) = previous {
                previous.close();
            }
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

            // The per-book cache dir the session extracts the book's
            // embedded (publisher) fonts into; `Library::remove` and the
            // open-time sweep clean it up with the book.
            let publisher_font_dir = library
                .data_dir()
                .join(inkuna_core::PUBLISHER_FONT_DIR)
                .join(&id);
            let session = inkuna_core::EngineSession::open(
                &epub_path,
                fonts.clone(),
                viewport.into(),
                settings.into(),
                publication.language.clone(),
                opening_chapter,
                Some(&publisher_font_dir),
                Arc::new(ListenerAdapter(listener)),
            )
            .map_err(|e| InkunaError::from(inkuna_core::CoreError::from(e)))?;
            // Store the new session; the NEWEST-TICKETED request wins
            // regardless of completion order. When a newer open was
            // requested meanwhile, `store` hands this session back and
            // it closes off to the side — one live reader either way,
            // and always the most recently requested one.
            if let Some(displaced) = active.store(ticket, &session) {
                displaced.close();
            }

            // The session's own registry: base blocks plus this book's
            // publisher block — what `font_registry()` must serve so the
            // shells can draw every display-list font id.
            let fonts = session.fonts();
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

/// The core's own crate version, for About screens and diagnostics. It
/// moves independently of the iOS and Android app versions the stores
/// show, so a bug report wants both. Free-standing on purpose: callable
/// without opening a [`Bookshelf`].
#[uniffi::export]
pub fn core_version() -> String {
    inkuna_core::version().to_string()
}

#[cfg(test)]
#[path = "bookshelf_tests.rs"]
mod tests;
