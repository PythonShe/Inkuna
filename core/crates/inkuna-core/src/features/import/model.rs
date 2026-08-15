//! What an import resolved to, per file.

use crate::{CoreError, Publication};

/// What importing one file resolved to. Only these two outcomes are
/// non-exceptional; anything else is a `CoreError`.
#[derive(Debug, Clone, PartialEq)]
pub enum ImportOutcome {
    /// The file was copied into core-owned storage and its rows committed.
    Imported(Publication),
    /// Content-hash dedupe matched an existing publication: nothing was
    /// added, the staged copy was discarded, and the carried publication is
    /// the one already in the library.
    Duplicate(Publication),
}

/// Per-item outcome of a batch import: failures are reported in place of
/// throwing so one bad file never aborts the rest of a multi-selection.
#[derive(Debug)]
pub enum BatchImportOutcome {
    Imported(Publication),
    Duplicate(Publication),
    Failed { path: String, error: CoreError },
}
