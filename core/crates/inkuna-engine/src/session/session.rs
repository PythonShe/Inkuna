//! The session object: open/close lifecycle, the shared state the
//! worker and the query surface both speak to, and the async-shaped
//! entry points (`update_layout`, `resource`).

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;

use inkuna_content::{read_package, read_resource, RenditionLayout};

use crate::error::EngineError;
use crate::fonts::FontRegistry;
use crate::settings::LayoutSettings;

use super::cache::Cache;
use super::model::{LayoutEvents, Viewport};
use super::worker;

/// Mutex-guarded session state.
pub(super) struct Inner {
    pub cache: Cache,
    /// Explicit work, front = highest priority (cache misses).
    pub queue: VecDeque<u32>,
    /// The most recently queried chapter — the worker prefetches its
    /// spine neighbors (±1, ±2) and the cache never evicts it.
    pub focus: u32,
    pub viewport: Viewport,
    pub settings: LayoutSettings,
}

/// State shared between the session handle and the worker thread.
pub(super) struct Shared {
    pub inner: Mutex<Inner>,
    pub work: Condvar,
    /// Bumped by `update_layout`; the worker abandons results whose
    /// generation is no longer current.
    pub generation: AtomicU64,
    pub closed: AtomicBool,
    pub events: Arc<dyn LayoutEvents>,
    pub epub_path: PathBuf,
    pub fonts: Arc<FontRegistry>,
    /// Spine hrefs in reading order, package-root-relative.
    pub spine: Vec<String>,
    pub rtl_progression: bool,
    /// Publication language for shaping (the `open` parameter, falling
    /// back to the package's `dc:language`).
    pub lang: Option<String>,
}

impl Shared {
    /// Panic-free lock: a poisoned mutex yields the inner state (the
    /// worker's panics are already caught and scoped per chapter).
    pub fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// One open publication with a background layout worker. All query
/// methods are synchronous, cache-only, and non-blocking: a miss
/// schedules the chapter at queue front and returns
/// [`EngineError::NotReady`] — never blocks.
pub struct EngineSession {
    pub(super) shared: Arc<Shared>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl EngineSession {
    /// Opens the publication and starts the worker: the opening chapter
    /// lays out first, then spine neighbors of the most recently
    /// queried chapter. A `pre-paginated` package is rejected —
    /// fixed-layout gets a dedicated navigator, never this engine.
    pub fn open(
        epub_path: &Path,
        fonts: Arc<FontRegistry>,
        viewport: Viewport,
        settings: LayoutSettings,
        lang: Option<String>,
        opening_chapter: u32,
        events: Arc<dyn LayoutEvents>,
    ) -> Result<Arc<EngineSession>, EngineError> {
        let package = read_package(epub_path)?;
        if package.rendition_layout == RenditionLayout::PrePaginated {
            return Err(EngineError::UnsupportedContent {
                detail: "fixed-layout".to_string(),
            });
        }
        let spine: Vec<String> = package.spine.iter().map(|s| s.href.clone()).collect();
        let focus = opening_chapter.min(spine.len().saturating_sub(1) as u32);
        let mut queue = VecDeque::new();
        if !spine.is_empty() {
            queue.push_back(focus);
        }
        let shared = Arc::new(Shared {
            inner: Mutex::new(Inner {
                cache: Cache::default(),
                queue,
                focus,
                viewport,
                settings: settings.clamped(),
            }),
            work: Condvar::new(),
            generation: AtomicU64::new(0),
            closed: AtomicBool::new(false),
            events,
            epub_path: epub_path.to_path_buf(),
            fonts,
            spine,
            rtl_progression: package.page_progression_rtl,
            lang: lang.or(package.metadata.language),
        });
        let worker_shared = Arc::clone(&shared);
        let handle = std::thread::Builder::new()
            .name("inkuna-engine-layout".to_string())
            .spawn(move || worker::run(worker_shared))?;
        Ok(Arc::new(EngineSession {
            shared,
            worker: Mutex::new(Some(handle)),
        }))
    }

    /// Closes the session: drains the queue, joins the worker. Every
    /// sync method on a closed session returns `NotReady`. Idempotent;
    /// also called by `Drop`.
    ///
    /// Pagination has no mid-chapter abort path, so `close()` may block
    /// for up to one chapter's layout while the worker finishes its
    /// current job (it publishes nothing and emits no events once the
    /// closed flag is set).
    ///
    /// Called ON the worker thread itself — e.g. the last
    /// `Arc<EngineSession>` dropped inside a [`LayoutEvents`] callback —
    /// it sets the closed flag and drains the queue but SKIPS the join:
    /// a thread joining itself deadlocks (std panics on it), and the
    /// worker loop already exits on the closed flag, detaching cleanly
    /// when the un-joined handle drops.
    pub fn close(&self) {
        {
            let mut inner = self.shared.lock();
            self.shared.closed.store(true, Ordering::Release);
            inner.queue.clear();
        }
        self.shared.work.notify_all();
        let mut guard = self.worker.lock().unwrap_or_else(PoisonError::into_inner);
        if guard
            .as_ref()
            .is_some_and(|h| h.thread().id() == std::thread::current().id())
        {
            return;
        }
        let handle = guard.take();
        drop(guard);
        if let Some(handle) = handle {
            if handle.join().is_err() {
                // Per-chapter panics are caught in the worker; this is
                // the never-expected backstop.
                log::error!("layout worker terminated abnormally");
            }
        }
    }

    /// Relayout under a new viewport/settings: bumps the generation,
    /// clears the queue, invalidates the cache, and schedules the
    /// current chapter first. Heavy path — the FFI calls it from a
    /// blocking pool.
    pub fn update_layout(&self, viewport: Viewport, settings: LayoutSettings) {
        if self.closed() {
            return;
        }
        {
            let mut inner = self.shared.lock();
            self.shared.generation.fetch_add(1, Ordering::AcqRel);
            inner.viewport = viewport;
            inner.settings = settings.clamped();
            inner.queue.clear();
            inner.cache.clear();
            let focus = inner.focus;
            if !self.shared.spine.is_empty() {
                inner.queue.push_back(focus);
            }
        }
        self.shared.work.notify_all();
    }

    /// One resource's bytes, budget-capped by the container layer.
    /// Heavy path — the FFI calls it from a blocking pool.
    pub fn resource(&self, href: &str) -> Result<Vec<u8>, EngineError> {
        if self.closed() {
            return Err(EngineError::NotReady);
        }
        Ok(read_resource(&self.shared.epub_path, href)?)
    }

    pub fn spine_len(&self) -> u32 {
        self.shared.spine.len() as u32
    }

    pub(super) fn closed(&self) -> bool {
        self.shared.closed.load(Ordering::Acquire)
    }
}

impl Drop for EngineSession {
    fn drop(&mut self) {
        self.close();
    }
}
