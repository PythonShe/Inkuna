//! FFI surface for the Inkuna core, exported via UniFFI to Swift and Kotlin.
//!
//! Types here mirror `inkuna-core` deliberately: the core stays pure Rust
//! (free to use references, borrows, rich enums) while this layer keeps the
//! boundary coarse-grained and owned-value only. File paths are stored
//! relative in the DB and absolutized here on read, because iOS container
//! paths change across installs.

use std::sync::Arc;

uniffi::setup_scaffolding!("inkuna");

/// Mirrors `CoreError` variant-for-variant, *structured*: no
/// `flat_error`, so the shells receive fields, not a Display string to
/// parse. `format` in `UnsupportedFormat` survives the boundary as the
/// value the core detected, and nothing downstream ever needs to
/// prefix-match an error message. The field is `detail` rather than
/// `message` because the generated Kotlin exception classes would
/// otherwise collide with `Throwable.message`.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum InkunaError {
    #[error("io error: {detail}")]
    Io { detail: String },
    #[error("file too large: over {limit} bytes")]
    FileTooLarge { limit: u64 },
    #[error("database error: {detail}")]
    Database { detail: String },
    #[error("archive error: {detail}")]
    Archive { detail: String },
    #[error("unsupported format{}", format.as_deref().map(|f| format!(": {f}")).unwrap_or_default())]
    UnsupportedFormat { format: Option<String> },
    #[error("invalid publication: {detail}")]
    InvalidPublication { detail: String },
    #[error("publication not found: {id}")]
    NotFound { id: String },
}

impl From<inkuna_core::CoreError> for InkunaError {
    fn from(e: inkuna_core::CoreError) -> Self {
        use inkuna_core::CoreError as C;
        match e {
            C::Io(e) => InkunaError::Io { detail: e.to_string() },
            C::FileTooLarge(limit) => InkunaError::FileTooLarge { limit },
            C::Database(e) => InkunaError::Database { detail: e.to_string() },
            C::Archive(m) => InkunaError::Archive { detail: m },
            C::UnsupportedFormat(f) => InkunaError::UnsupportedFormat { format: f },
            C::InvalidPublication(m) => InkunaError::InvalidPublication { detail: m },
            C::NotFound(id) => InkunaError::NotFound { id },
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Shelf {
    /// Opened at least once and not finished — "continue reading". A
    /// freshly imported book is deliberately absent.
    Reading,
    /// Everything not finished, opened or not: what a library screen lists,
    /// so an imported book is visible immediately.
    Unfinished,
    Finished,
    All,
}

impl From<Shelf> for inkuna_core::Shelf {
    fn from(s: Shelf) -> Self {
        match s {
            Shelf::Reading => inkuna_core::Shelf::Reading,
            Shelf::Unfinished => inkuna_core::Shelf::Unfinished,
            Shelf::Finished => inkuna_core::Shelf::Finished,
            Shelf::All => inkuna_core::Shelf::All,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Sort {
    /// Tonight's hero is the first row.
    RecentlyOpened,
    RecentlyAdded,
}

impl From<Sort> for inkuna_core::Sort {
    fn from(s: Sort) -> Self {
        match s {
            Sort::RecentlyOpened => inkuna_core::Sort::RecentlyOpened,
            Sort::RecentlyAdded => inkuna_core::Sort::RecentlyAdded,
        }
    }
}

/// `file_path`/`cover_path` are absolute here (relative in the DB).
#[derive(Debug, Clone, uniffi::Record)]
pub struct Publication {
    pub id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub language: Option<String>,
    pub format: Format,
    pub file_path: String,
    pub cover_path: Option<String>,
    pub added_at: i64,
    /// Book-wide `totalProgression` in [0, 1].
    pub progression: f64,
    /// Opaque Readium locator JSON for the current position.
    pub locator: Option<String>,
    /// Readium synthetic position count, once the shell reported it.
    pub position_count: Option<u32>,
    pub finished_at: Option<i64>,
    pub last_opened_at: Option<i64>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct Chapter {
    pub id: String,
    pub idx: u32,
    pub title: String,
    /// Package-root-relative Readium jump target, possibly with fragment.
    pub href: String,
    pub depth: u32,
}

impl From<inkuna_core::Chapter> for Chapter {
    fn from(c: inkuna_core::Chapter) -> Self {
        Chapter {
            id: c.id,
            idx: c.idx,
            title: c.title,
            href: c.href,
            depth: c.depth,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct Bookmark {
    pub id: String,
    pub publication_id: String,
    /// Opaque Readium locator JSON; carries the row's display context.
    pub locator: String,
    pub progression: f64,
    pub created_at: i64,
}

impl From<inkuna_core::Bookmark> for Bookmark {
    fn from(b: inkuna_core::Bookmark) -> Self {
        Bookmark {
            id: b.id,
            publication_id: b.publication_id,
            locator: b.locator,
            progression: b.progression,
            created_at: b.created_at,
        }
    }
}

/// Numbers only — shells own formatting and localization.
#[derive(Debug, Clone, uniffi::Record)]
pub struct StatsOverview {
    pub pages_this_week: u32,
    pub minutes_this_month: u32,
    pub books_finished_this_year: u32,
    /// Day-of-month numbers of the current local month with ≥1 session.
    pub read_days: Vec<u8>,
}

impl From<inkuna_core::StatsOverview> for StatsOverview {
    fn from(s: inkuna_core::StatsOverview) -> Self {
        StatsOverview {
            pages_this_week: s.pages_this_week,
            minutes_this_month: s.minutes_this_month,
            books_finished_this_year: s.books_finished_this_year,
            read_days: s.read_days,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct Settings {
    pub onboarded: bool,
    /// Opaque theme identifier; shells own the palettes.
    pub reading_theme: String,
    pub text_size_step: u8,
    pub brightness: f64,
}

impl From<inkuna_core::Settings> for Settings {
    fn from(s: inkuna_core::Settings) -> Self {
        Settings {
            onboarded: s.onboarded,
            reading_theme: s.reading_theme,
            text_size_step: s.text_size_step,
            brightness: s.brightness,
        }
    }
}

impl From<Settings> for inkuna_core::Settings {
    fn from(s: Settings) -> Self {
        inkuna_core::Settings {
            onboarded: s.onboarded,
            reading_theme: s.reading_theme,
            text_size_step: s.text_size_step,
            brightness: s.brightness,
        }
    }
}

#[derive(Debug, uniffi::Enum)]
pub enum ImportOutcome {
    Imported { publication: Publication },
    /// The library already holds this content; nothing was added.
    Duplicate { publication: Publication },
    /// Batch-only: one bad file never aborts the rest of a selection.
    /// Carries the same typed error the single-file path throws, so the
    /// two paths classify failures identically.
    Failed { path: String, error: InkunaError },
}

/// One open file descriptor to import. Android's SAF hands out streams,
/// not paths; passing the descriptor itself lets the core's copy into its
/// own storage be the only copy, with no shell-side staging layer.
#[derive(Debug, uniffi::Record)]
pub struct FdImport {
    /// An open, readable file descriptor. **Ownership transfers with the
    /// call**: the shell must detach it first (Android's
    /// `ParcelFileDescriptor.detachFd()`), and the core reads and closes
    /// it exactly once, success or failure. It need not be seekable — a
    /// provider pipe works.
    pub fd: i32,
    /// The provider's name for the document, verbatim (CJK included). It
    /// stands in for the filename everywhere one is needed: the TXT
    /// extension check, the fallback title, failure reporting, and
    /// progress events.
    pub display_name: String,
}

/// Observes a batch import while it runs. Implemented by the shells;
/// called from Rust worker threads, so implementations must hop to their
/// own main thread before touching UI.
#[uniffi::export(with_foreign)]
pub trait ImportProgressListener: Send + Sync {
    /// One file finished — imported, duplicate, or failed. `completed`
    /// counts finished files including this one and is strictly
    /// increasing across calls; `path` names the input that finished,
    /// which is not necessarily the batch's next one, because files
    /// finish in parallel.
    fn on_file_complete(&self, completed: u32, total: u32, path: String);
}

/// Absolutizes DB-relative paths against the library's data dir.
fn publication_record(library: &inkuna_core::Library, p: inkuna_core::Publication) -> Publication {
    let data_dir = library.data_dir();
    Publication {
        file_path: data_dir.join(&p.file_path).to_string_lossy().into_owned(),
        cover_path: p
            .cover_path
            .as_deref()
            .map(|c| data_dir.join(c).to_string_lossy().into_owned()),
        id: p.id,
        title: p.title,
        authors: p.authors,
        language: p.language,
        format: p.format.into(),
        added_at: p.added_at,
        progression: p.progression,
        locator: p.locator,
        position_count: p.position_count,
        finished_at: p.finished_at,
        last_opened_at: p.last_opened_at,
    }
}

/// Maps a core batch outcome into the FFI record, absolutizing paths.
fn batch_record(
    library: &inkuna_core::Library,
    outcome: inkuna_core::BatchImportOutcome,
) -> ImportOutcome {
    match outcome {
        inkuna_core::BatchImportOutcome::Imported(p) => ImportOutcome::Imported {
            publication: publication_record(library, p),
        },
        inkuna_core::BatchImportOutcome::Duplicate(p) => ImportOutcome::Duplicate {
            publication: publication_record(library, p),
        },
        inkuna_core::BatchImportOutcome::Failed { path, error } => ImportOutcome::Failed {
            path,
            error: error.into(),
        },
    }
}

/// Adopts a descriptor the shell detached for us.
///
/// SAFETY: `FdImport`'s contract is that the shell hands over sole
/// ownership; from here the `File` reads and closes it exactly once.
fn file_from(fd: i32) -> std::fs::File {
    use std::os::fd::FromRawFd;
    unsafe { std::fs::File::from_raw_fd(fd) }
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
    ///
    /// Hold exactly one `Bookshelf` per `data_dir` for the process lifetime:
    /// opening sweeps files no row references, so a second concurrent
    /// instance on the same directory deletes the first one's in-flight
    /// import.
    #[uniffi::constructor]
    pub fn open(data_dir: String) -> Result<Arc<Self>, InkunaError> {
        Ok(Arc::new(Bookshelf(Arc::new(inkuna_core::Library::open(&data_dir)?))))
    }

    /// Imports one file: copied into core-owned storage, hashed for
    /// dedupe, parsed for metadata/TOC/cover/text. Never returns `Failed`
    /// — hard I/O failures throw instead.
    pub async fn import(&self, path: String) -> Result<ImportOutcome, InkunaError> {
        let library = self.0.clone();
        blocking(move || {
            Ok(match library.import(&path)? {
                inkuna_core::ImportOutcome::Imported(p) => ImportOutcome::Imported {
                    publication: publication_record(&library, p),
                },
                inkuna_core::ImportOutcome::Duplicate(p) => ImportOutcome::Duplicate {
                    publication: publication_record(&library, p),
                },
            })
        })
        .await
    }

    /// Imports many files (document pickers multi-select), parallelizing
    /// the parse stage; per-item failures come back as `Failed` items in
    /// input order instead of throwing. `listener`, when given, hears one
    /// event per finished file while the call runs.
    pub async fn import_batch(
        &self,
        paths: Vec<String>,
        listener: Option<Arc<dyn ImportProgressListener>>,
    ) -> Result<Vec<ImportOutcome>, InkunaError> {
        let library = self.0.clone();
        blocking(move || {
            let total = paths.len() as u32;
            Ok(library
                .import_batch_with(&paths, &|done, path| {
                    if let Some(listener) = &listener {
                        listener.on_file_complete(done as u32, total, path.to_string());
                    }
                })
                .into_iter()
                .map(|outcome| batch_record(&library, outcome))
                .collect())
        })
        .await
    }

    /// [`import`](Self::import) over an open file descriptor — see
    /// [`FdImport`] for the ownership contract. Never returns `Failed`;
    /// hard failures throw instead.
    pub async fn import_fd(&self, item: FdImport) -> Result<ImportOutcome, InkunaError> {
        let library = self.0.clone();
        blocking(move || {
            let mut file = file_from(item.fd);
            Ok(match library.import_reader(&mut file, &item.display_name)? {
                inkuna_core::ImportOutcome::Imported(p) => ImportOutcome::Imported {
                    publication: publication_record(&library, p),
                },
                inkuna_core::ImportOutcome::Duplicate(p) => ImportOutcome::Duplicate {
                    publication: publication_record(&library, p),
                },
            })
        })
        .await
    }

    /// [`import_batch`](Self::import_batch) over open file descriptors.
    /// Outcomes come back in input order; `Failed.path` and progress
    /// events carry each item's `display_name`, the only name a stream
    /// has. Every descriptor is closed by this call, whatever happens.
    pub async fn import_batch_fds(
        &self,
        items: Vec<FdImport>,
        listener: Option<Arc<dyn ImportProgressListener>>,
    ) -> Result<Vec<ImportOutcome>, InkunaError> {
        let library = self.0.clone();
        blocking(move || {
            let total = items.len() as u32;
            // Adopt every descriptor up front, before any work can fail,
            // so each one is closed exactly once no matter what follows.
            let items: Vec<(std::fs::File, String)> = items
                .into_iter()
                .map(|item| (file_from(item.fd), item.display_name))
                .collect();
            Ok(library
                .import_batch_readers(items, &|done, name| {
                    if let Some(listener) = &listener {
                        listener.on_file_complete(done as u32, total, name.to_string());
                    }
                })
                .into_iter()
                .map(|outcome| batch_record(&library, outcome))
                .collect())
        })
        .await
    }

    pub async fn list(&self, shelf: Shelf, sort: Sort) -> Result<Vec<Publication>, InkunaError> {
        let library = self.0.clone();
        blocking(move || {
            let publications = library.list(shelf.into(), sort.into())?;
            Ok(publications
                .into_iter()
                .map(|p| publication_record(&library, p))
                .collect())
        })
        .await
    }

    pub async fn publication(&self, id: String) -> Result<Publication, InkunaError> {
        let library = self.0.clone();
        blocking(move || {
            let publication = library.publication(&id)?;
            Ok(publication_record(&library, publication))
        })
        .await
    }

    /// Removes the publication row (bookmarks, sessions, chapters, and
    /// text cascade), its book file, and its cover.
    pub async fn remove(&self, id: String) -> Result<(), InkunaError> {
        let library = self.0.clone();
        blocking(move || Ok(library.remove(&id)?)).await
    }

    /// Case-folded, CJK-safe substring search over title + authors.
    pub async fn search_library(&self, query: String) -> Result<Vec<Publication>, InkunaError> {
        let library = self.0.clone();
        blocking(move || {
            let publications = library.search_library(&query)?;
            Ok(publications
                .into_iter()
                .map(|p| publication_record(&library, p))
                .collect())
        })
        .await
    }

    /// One call per page turn. `locator` is the opaque Readium locator
    /// JSON; `progression` the book-wide totalProgression; `position` the
    /// synthetic position, once the navigator knows it.
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

    /// Once per book, after the navigator computes synthetic positions;
    /// from then on "page N of M" is real.
    pub async fn report_position_count(&self, id: String, count: u32) -> Result<(), InkunaError> {
        let library = self.0.clone();
        blocking(move || Ok(library.report_position_count(&id, count)?)).await
    }

    /// Explicit finish/unfinish; unfinishing sticks at end-of-book because
    /// auto-finish only fires on an upward crossing of the threshold.
    pub async fn set_finished(&self, id: String, finished: bool) -> Result<(), InkunaError> {
        let library = self.0.clone();
        blocking(move || Ok(library.set_finished(&id, finished)?)).await
    }

    /// The flattened TOC in document order; empty when the book has none.
    pub async fn chapters(&self, id: String) -> Result<Vec<Chapter>, InkunaError> {
        let library = self.0.clone();
        blocking(move || Ok(library.chapters(&id)?.into_iter().map(Into::into).collect())).await
    }

    pub async fn add_bookmark(
        &self,
        id: String,
        locator: String,
        progression: f64,
    ) -> Result<Bookmark, InkunaError> {
        let library = self.0.clone();
        blocking(move || Ok(library.add_bookmark(&id, &locator, progression)?.into())).await
    }

    /// Bookmarks sorted by progression through the book.
    pub async fn bookmarks(&self, id: String) -> Result<Vec<Bookmark>, InkunaError> {
        let library = self.0.clone();
        blocking(move || Ok(library.bookmarks(&id)?.into_iter().map(Into::into).collect())).await
    }

    pub async fn remove_bookmark(&self, bookmark_id: String) -> Result<(), InkunaError> {
        let library = self.0.clone();
        blocking(move || Ok(library.remove_bookmark(&bookmark_id)?)).await
    }

    /// Starts a reading session at reader open; returns the session id.
    /// Also closes any crashed session for this publication retroactively.
    pub async fn session_start(&self, id: String) -> Result<String, InkunaError> {
        let library = self.0.clone();
        blocking(move || Ok(library.session_start(&id)?)).await
    }

    /// Ends a session at reader close / app background. Idempotent.
    pub async fn session_end(&self, session_id: String) -> Result<(), InkunaError> {
        let library = self.0.clone();
        blocking(move || Ok(library.session_end(&session_id)?)).await
    }

    /// Stats screen numbers in the caller's local time.
    ///
    /// `tz_offset_minutes` is **minutes** east of UTC, accepted range
    /// −1439..=1439 (e.g. 540 for JST, −480 for PST). An offset outside
    /// that range is not an error — the numbers are silently bucketed in
    /// UTC instead — so passing seconds by mistake yields plausible but
    /// wrong buckets rather than a failure.
    pub async fn stats_overview(
        &self,
        tz_offset_minutes: i32,
        week_starts_monday: bool,
    ) -> Result<StatsOverview, InkunaError> {
        let library = self.0.clone();
        blocking(move || {
            Ok(library
                .stats_overview(tz_offset_minutes, week_starts_monday)?
                .into())
        })
        .await
    }

    pub async fn settings(&self) -> Result<Settings, InkunaError> {
        let library = self.0.clone();
        blocking(move || Ok(library.settings()?.into())).await
    }

    /// Whole-record write; the core clamps out-of-range values.
    pub async fn set_settings(&self, settings: Settings) -> Result<(), InkunaError> {
        let library = self.0.clone();
        blocking(move || Ok(library.set_settings(&settings.into())?)).await
    }
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, InkunaError> + Send + 'static,
) -> Result<T, InkunaError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| InkunaError::Io { detail: format!("task join error: {e}") })?
}

#[uniffi::export]
pub fn core_version() -> String {
    inkuna_core::version().to_string()
}
