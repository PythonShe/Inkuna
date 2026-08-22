//! The layout-event listener the shells implement, plus the private
//! adapter that feeds it from the engine's `LayoutEvents`.

use std::sync::Arc;

/// Observes layout progress while a reader session's worker runs.
/// Implemented by the shells; callbacks arrive on engine threads, so
/// implementations must hop to their own main thread before touching
/// UI. A chapter that FAILS layout emits no event at all — the failure
/// signal is the query path, which returns the terminal error instead
/// of `NotReady` once the worker caches it.
#[uniffi::export(with_foreign)]
pub trait LayoutListener: Send + Sync {
    /// Page 0 of `spine_idx` is available — the first-paint moment.
    fn on_first_page_ready(&self, generation: u64, spine_idx: u32);
    /// The chapter finished laying out with `page_count` pages.
    fn on_chapter_ready(&self, generation: u64, spine_idx: u32, page_count: u32);
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
}
