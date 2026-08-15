//! Inkuna core: format-agnostic publication library.
//!
//! Owns everything that is not pixels: the library database, import
//! pipeline, format detection, metadata extraction, and reading progress.
//! Rendering stays in the platform shells (Readium navigators).

mod core;
mod formats;
mod import;
mod library;
mod progress;
mod settings;
mod stats;

pub use crate::core::error::CoreError;
pub use formats::Format;
pub use import::{BatchImportOutcome, ImportOutcome};
pub use library::{Bookmark, Chapter, Library, Publication, Shelf, Sort};
pub use settings::Settings;
pub use stats::StatsOverview;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
