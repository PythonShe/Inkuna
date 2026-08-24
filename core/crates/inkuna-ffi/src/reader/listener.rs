//! The layout-event listener the shells implement, plus the private
//! adapter that feeds it from the engine's `LayoutEvents`.

use std::sync::Arc;

/// Observes layout progress while a reader session's worker runs.
/// Implemented by the shells; callbacks arrive on engine threads, so
/// implementations must hop to their own main thread before touching
/// UI.
///
/// Every terminal outcome of laying a chapter out has exactly one
/// event, so a shell that shows a loading state is always woken: page 0
/// by `on_first_page_ready`, completion by `on_chapter_ready`, and a
/// chapter that fails closed by `on_chapter_failed`. Shells never poll.
#[uniffi::export(with_foreign)]
pub trait LayoutListener: Send + Sync {
    /// Page 0 of `spine_idx` is available — the first-paint moment.
    fn on_first_page_ready(&self, generation: u64, spine_idx: u32);
    /// The chapter finished laying out with `page_count` pages.
    fn on_chapter_ready(&self, generation: u64, spine_idx: u32, page_count: u32);
    /// The chapter failed closed and has no page and never will at this
    /// generation. Not a readiness signal: from here `chapter()` and
    /// `page()` on `spine_idx` throw `UnsupportedContent` instead of
    /// `NotReady`, so the shell replaces its loading state with the
    /// unreadable-chapter placeholder for that spine slot. The rest of
    /// the book stays navigable.
    fn on_chapter_failed(&self, generation: u64, spine_idx: u32);
}

/// Adapts a shell listener onto the engine's callback trait.
pub(crate) struct ListenerAdapter(pub(crate) Arc<dyn LayoutListener>);

impl inkuna_core::LayoutEvents for ListenerAdapter {
    fn first_page_ready(&self, generation: u64, spine_idx: u32) {
        self.0.on_first_page_ready(generation, spine_idx);
    }

    fn chapter_ready(&self, generation: u64, spine_idx: u32, page_count: u32) {
        self.0.on_chapter_ready(generation, spine_idx, page_count);
    }

    fn chapter_failed(&self, generation: u64, spine_idx: u32) {
        self.0.on_chapter_failed(generation, spine_idx);
    }
}
