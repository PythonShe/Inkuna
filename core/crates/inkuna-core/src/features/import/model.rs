//! What an import resolved to, per file.

use crate::{CoreError, Publication};

#[derive(Debug, Clone, PartialEq)]
pub enum ImportOutcome {
    Imported(Publication),
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
