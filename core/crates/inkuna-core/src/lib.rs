//! Inkuna core: format-agnostic publication library.
//!
//! Owns everything that is not pixels: the library database, import
//! pipeline, format detection, metadata extraction, and reading progress.
//! Rendering stays in the platform shells (Readium navigators).

mod epub;
mod error;
mod format;
mod import;
mod library;
mod migrate;
mod pool;

pub use error::CoreError;
pub use format::Format;
pub use import::{BatchImportOutcome, ImportOutcome};
pub use library::{Chapter, Library, Publication};

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
