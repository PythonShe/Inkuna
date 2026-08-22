//! The single layout worker: a priority loop that lays chapters out and
//! publishes each page into the cache AS EMITTED — page 0 is queryable
//! long before the chapter completes. Panics never kill the session:
//! each chapter runs under `catch_unwind` and a panic caches the
//! chapter as failed (panics remain forbidden; this is the backstop).

use std::panic::AssertUnwindSafe;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use inkuna_content::{read_resource, resolve_relative, split_fragment};

use crate::display::{build_page, DisplayContext};
use crate::dom::{parse, StylesheetSource, MAX_STYLESHEET_BYTES};
use crate::fixed::Fx;
use crate::paginate::{paginate, ChapterInput, FxSize};
use crate::settings::LayoutSettings;
use crate::style::{cap_sheet_sources, parse_sheet, resolve};
use crate::text::project;

use super::cache::SlotState;
use super::model::Viewport;
use super::session::{Inner, Shared};

struct Job {
    spine_idx: u32,
    generation: u64,
    href: String,
    viewport: Viewport,
    settings: LayoutSettings,
}

/// The worker loop: runs until the session closes.
pub(super) fn run(shared: Arc<Shared>) {
    while let Some(job) = next_job(&shared) {
        let outcome = std::panic::catch_unwind(AssertUnwindSafe(|| lay_chapter(&shared, &job)));
        if outcome.is_err() {
            log::error!("layout panicked for spine {}", job.spine_idx);
            publish_failed(&shared, &job, "layout panicked".to_string());
        }
    }
}

/// Blocks until work exists or the session closes. Priority: explicit
/// schedules (cache misses, queue front first), then spine neighbors
/// (±1, ±2) of the focused chapter.
fn next_job(shared: &Shared) -> Option<Job> {
    let mut inner = shared.lock();
    loop {
        if shared.closed.load(Ordering::Acquire) {
            return None;
        }
        if let Some(job) = pick(shared, &mut inner) {
            return Some(job);
        }
        inner = shared
            .work
            .wait(inner)
            .unwrap_or_else(std::sync::PoisonError::into_inner);
    }
}

fn pick(shared: &Shared, inner: &mut Inner) -> Option<Job> {
    let generation = shared.generation.load(Ordering::Acquire);
    let spine_len = shared.spine.len() as i64;
    let mut candidate = None;
    while let Some(idx) = inner.queue.pop_front() {
        if i64::from(idx) < spine_len && !inner.cache.contains(idx, generation) {
            candidate = Some(idx);
            break;
        }
    }
    if candidate.is_none() {
        // Prefetch radius matches the cache: the focus and its ±2
        // neighbors are exactly what the cache can hold.
        for d in [0i64, 1, -1, 2, -2] {
            let idx = i64::from(inner.focus) + d;
            if idx >= 0 && idx < spine_len && !inner.cache.contains(idx as u32, generation) {
                candidate = Some(idx as u32);
                break;
            }
        }
    }
    let spine_idx = candidate?;
    inner.cache.insert_laying(spine_idx, generation);
    Some(Job {
        spine_idx,
        generation,
        href: shared.spine[spine_idx as usize].clone(),
        viewport: inner.viewport,
        settings: inner.settings.clone(),
    })
}

/// True when the job's results are stale: a newer generation exists or
/// the session closed. Checked before every publish.
fn stale(shared: &Shared, job: &Job) -> bool {
    shared.closed.load(Ordering::Acquire)
        || shared.generation.load(Ordering::Acquire) != job.generation
}

/// Lays one chapter out, publishing progressively.
fn lay_chapter(shared: &Shared, job: &Job) {
    let bytes = match read_resource(&shared.epub_path, &job.href) {
        Ok(bytes) => bytes,
        Err(e) => return publish_failed(shared, job, e.to_string()),
    };
    let doc = match parse(&bytes) {
        Ok(doc) => doc,
        Err(e) => return publish_failed(shared, job, e.to_string()),
    };

    // Stylesheets in document order: linked sheets read through the
    // container layer (resolved against the chapter's own path;
    // unreadable → skipped, logged), inline bodies verbatim. The
    // cascade list drops whole sheets from the END past the byte cap.
    let mut css: Vec<String> = Vec::new();
    for source in &doc.stylesheets {
        match source {
            StylesheetSource::Inline(text) => css.push(text.clone()),
            StylesheetSource::Linked(href) => {
                let resolved = resolve_relative(&job.href, href);
                let (path, _) = split_fragment(&resolved);
                match read_resource(&shared.epub_path, path) {
                    Ok(bytes) => css.push(String::from_utf8_lossy(&bytes).into_owned()),
                    Err(e) => log::warn!("stylesheet skipped: {path} ({e})"),
                }
            }
        }
    }
    let refs: Vec<&str> = css.iter().map(String::as_str).collect();
    let sheets: Vec<_> = cap_sheet_sources(&refs, MAX_STYLESHEET_BYTES)
        .iter()
        .map(|c| parse_sheet(c))
        .collect();

    let styled = resolve(&doc, &sheets);
    let projection = project(&styled);

    // Projection facts publish before pagination: anchors, text, and
    // writing mode are queryable as soon as they exist.
    {
        let mut inner = shared.lock();
        if stale(shared, job) {
            return;
        }
        let Some(SlotState::Laying(data)) =
            inner.cache.state_mut(job.spine_idx, job.generation)
        else {
            return;
        };
        data.text = projection.text.clone();
        data.anchors = projection.anchors.clone();
        data.writing_mode = styled.writing_mode;
        data.truncated = projection.truncated;
    }

    let settings = job.settings.clone().clamped();
    let typography = settings.typography();
    let viewport = FxSize {
        width: Fx::from_pt(job.viewport.width),
        height: Fx::from_pt(job.viewport.height),
    };
    let lang = shared.lang.as_deref();
    let resources = |href: &str| read_resource(&shared.epub_path, href).ok();
    let input = ChapterInput {
        styled: &styled,
        projection: &projection,
        fonts: &shared.fonts,
        typography: &typography,
        settings: &settings,
        viewport,
        lang,
        resource_path: &job.href,
        resources: &resources,
    };
    let ctx = DisplayContext::new(
        job.generation,
        &styled,
        &projection,
        &shared.fonts,
        viewport,
        &job.href,
    );

    let result = paginate(&input, &mut |page| {
        // Stale results are discarded page by page; pagination itself
        // runs to completion (it has no abort path) but publishes
        // nothing further.
        if stale(shared, job) {
            return;
        }
        let built = build_page(&page, &ctx);
        let first = page.index == 0;
        let published = {
            let mut inner = shared.lock();
            match inner.cache.state_mut(job.spine_idx, job.generation) {
                Some(SlotState::Laying(data)) => {
                    data.pages.push(built);
                    true
                }
                _ => false,
            }
        };
        if published && first {
            shared.events.first_page_ready(job.generation, job.spine_idx);
        }
    });

    match result {
        Ok(res) => {
            let published = {
                let mut inner = shared.lock();
                if stale(shared, job) {
                    return;
                }
                match inner.cache.state_mut(job.spine_idx, job.generation) {
                    Some(state) => {
                        // Two-step swap keeps this panic-free: only a
                        // Laying slot completes; anything else is put
                        // back untouched.
                        let old = std::mem::replace(state, SlotState::Failed(String::new()));
                        match old {
                            SlotState::Laying(mut data) => {
                                data.char_len = res.char_len;
                                data.truncated = res.truncated;
                                data.complete = true;
                                let page_count = data.pages.len() as u32;
                                *state = SlotState::Ready(data);
                                let focus = inner.focus;
                                inner.cache.evict(focus);
                                Some(page_count)
                            }
                            other => {
                                *state = other;
                                None
                            }
                        }
                    }
                    None => None,
                }
            };
            if let Some(page_count) = published {
                shared
                    .events
                    .chapter_ready(job.generation, job.spine_idx, page_count);
            }
        }
        Err(e) => publish_failed(shared, job, e.to_string()),
    }
}

/// Caches the chapter as failed-closed, scoped to the resource.
fn publish_failed(shared: &Shared, job: &Job, detail: String) {
    log::error!("chapter {} failed closed: {detail}", job.spine_idx);
    let mut inner = shared.lock();
    if stale(shared, job) {
        return;
    }
    if let Some(state) = inner.cache.state_mut(job.spine_idx, job.generation) {
        *state = SlotState::Failed(detail);
    }
    let focus = inner.focus;
    inner.cache.evict(focus);
}
