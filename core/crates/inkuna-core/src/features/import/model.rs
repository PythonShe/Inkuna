//! What an import resolved to, per file.

use crate::{CoreError, Publication};

/// What importing one file resolved to. Only these outcomes are
/// non-exceptional; anything else is a `CoreError`.
#[derive(Debug, Clone, PartialEq)]
pub enum ImportOutcome {
    /// The file was copied into core-owned storage and its rows committed.
    Imported(Publication),
    /// Content-hash dedupe matched an existing publication: nothing was
    /// added, the staged copy was discarded, and the carried publication is
    /// the one already in the library.
    Duplicate(Publication),
    /// Content-hash dedupe matched a *removed* publication. The file was
    /// imported in full — new copy, cover, spine, TOC, corpus, search docs
    /// — onto the tombstone's own id, so the reading history kept at
    /// removal (sessions, bookmarks, progression, finished state) is
    /// attached to it again.
    Restored {
        publication: Publication,
        /// Whether the exact reading position and bookmark coordinates
        /// survived. `false` when the canonical text projection changed
        /// between the removal and now: the coordinates stored then no
        /// longer address the same characters, so they were dropped and
        /// the book falls back to its (still accurate) `progression`.
        /// A shell may tell the reader their exact spot could not be
        /// recovered; it never means the import failed.
        coordinates_restored: bool,
    },
}

/// Per-item outcome of a batch import: failures are reported in place of
/// throwing so one bad file never aborts the rest of a multi-selection.
#[derive(Debug)]
pub enum BatchImportOutcome {
    Imported(Publication),
    Duplicate(Publication),
    Restored {
        publication: Publication,
        coordinates_restored: bool,
    },
    Failed {
        path: String,
        error: CoreError,
    },
}

impl From<ImportOutcome> for BatchImportOutcome {
    fn from(outcome: ImportOutcome) -> Self {
        match outcome {
            ImportOutcome::Imported(p) => BatchImportOutcome::Imported(p),
            ImportOutcome::Duplicate(p) => BatchImportOutcome::Duplicate(p),
            ImportOutcome::Restored {
                publication,
                coordinates_restored,
            } => BatchImportOutcome::Restored {
                publication,
                coordinates_restored,
            },
        }
    }
}
