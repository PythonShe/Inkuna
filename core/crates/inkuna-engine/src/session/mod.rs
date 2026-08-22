//! The engine session: one open publication, laid out chapter by
//! chapter on a single background worker thread, queried synchronously
//! from a cache that never blocks.
//!
//! DETERMINISM: page content is a pure function of `(resource bytes,
//! viewport, settings fingerprint, font set, engine version)` — the
//! worker thread only decides WHEN a page becomes available, never what
//! it contains. Nothing thread-timing-dependent ever reaches page
//! content.

mod cache;
mod model;
mod queries;
mod session;
mod worker;

#[cfg(test)]
mod tests;

pub use model::{
    ChapterGeometry, CharRange, HitResult, LayoutEvents, PageLocation, SelectionRect, Viewport,
};
pub use session::EngineSession;
