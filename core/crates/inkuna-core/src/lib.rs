//! Inkuna core: format-agnostic publication library.
//!
//! Owns everything that is not pixels: the library database, import
//! pipeline, format detection, metadata extraction, and reading progress.
//! Rendering stays in the platform shells (Readium navigators).

mod core;
mod features;
mod formats;

#[cfg(test)]
mod test_support;

pub use crate::core::error::CoreError;
/// Re-exported for the stats API's `week_start` parameter, so consumers
/// never need a direct chrono dependency.
pub use chrono::Weekday;
pub use features::import::{BatchImportOutcome, ImportOutcome};
pub use features::library::{Bookmark, Chapter, Library, Publication, Shelf, Sort};
pub use features::progress::ChapterPositionRange;
pub use features::search::{BookSearchHit, BookSearchResults, LibrarySearchHit};
pub use features::settings::Settings;
pub use features::stats::StatsOverview;
pub use formats::Format;

/// The core's own crate version (`CARGO_PKG_VERSION`), which the shells
/// surface in About screens and diagnostics. It moves independently of the
/// iOS and Android app versions.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
