//! FFI surface for the Inkuna core, exported via UniFFI to Swift and Kotlin.
//!
//! Types here mirror `inkuna-core` deliberately: the core stays pure Rust
//! (free to use references, borrows, rich enums) while this layer keeps the
//! boundary coarse-grained and owned-value only. File paths are stored
//! relative in the DB and absolutized here on read, because iOS container
//! paths change across installs.
//!
//! Modules mirror the core's feature slices; each contributes its own
//! `impl Bookshelf` block, the way core features contribute `impl Library`
//! blocks.

uniffi::setup_scaffolding!("inkuna");

mod bookshelf;
mod error;
mod format;
mod import;
mod library;
mod progress;
mod search;
mod settings;
mod stats;

pub use bookshelf::{core_version, Bookshelf};
pub use error::InkunaError;
pub use format::Format;
pub use import::{FdImport, ImportOutcome, ImportProgressListener};
pub use library::{Bookmark, Chapter, Publication, Shelf, Sort};
pub use progress::ChapterPositionRange;
pub use search::{BookSearchHit, BookSearchResults, LibrarySearchHit};
pub use settings::Settings;
pub use stats::StatsOverview;
