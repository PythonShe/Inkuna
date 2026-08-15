//! Aggregate persistence circuit breaker for one publication's commit.
//!
//! Defense in depth behind the parse-time caps (`MAX_TOC_ENTRIES`,
//! `MAX_SPINE_ITEMS`, `MAX_TOC_TOTAL_BYTES`, ...): those bound each part
//! individually, but this branch has seen five distinct amplifications
//! where a value cloned inside an attacker-controlled loop slipped past
//! every per-part cap and imported "successfully" into hundreds of MB of
//! persistent rows. The caps can never be proven complete, so the commit
//! charges one meter per publication and converts any remaining door of
//! that class from *silent success with a mangled library* into *clean
//! failure with a rollback*.
//!
//! Scope is persistence, not memory: parse products are built before any
//! row is written, so parse-time bombs that never reach the DB (the
//! 616 MB manifest RSS spike, for one) are out of reach here — the
//! parse-time caps remain the only defense for those. This meter bounds
//! only what one `commit_import` writes.

use crate::CoreError;

/// Hard ceiling on rows one publication may insert across `publications`,
/// `chapters`, `resources`, and `resource_text`.
///
/// Worst combination the parse-time caps admit: 1 publications row
/// + `MAX_TOC_ENTRIES` (100,000) chapters + `MAX_SPINE_ITEMS` (10,000)
///   resources + at most one `resource_text` per resource (10,000)
///
/// = 120,001 rows. 150,000 keeps ~25% headroom above that, so no book
/// the caps pass — honest or not — can reach it today; tripping it means
/// a row source the caps never saw.
pub(super) const MAX_PERSISTED_ROWS: u64 = 150_000;

/// Hard ceiling on the variable-length bytes one publication may persist
/// (titles, authors, hrefs, extracted text — the values a crafted file
/// controls; fixed-width ids and integers are bounded by
/// [`MAX_PERSISTED_ROWS`]).
///
/// Worst combination the parse-time caps admit:
/// chapters ≤ `MAX_TOC_TOTAL_BYTES` (8 MiB)
/// + resource hrefs ≤ `MAX_SPINE_ITEMS` × `MAX_HREF_BYTES`
///   (10,000 × 4,096 B ≈ 39.1 MiB — the spine dedupes on resolved href,
///   so this now takes 10,000 *distinct* ~4 KB hrefs, not one repeated)
/// + text ≤ `MAX_TOTAL_TEXT_BYTES` (32 MiB)
/// + publication metadata ≤ (1 title + `MAX_AUTHORS` (1,000) creators
///   + 1 language) × `MAX_METADATA_VALUE_BYTES` (2,048 B) ≈ 2 MiB
///
/// ≈ 81 MiB. 128 MiB keeps ~58% headroom above that, so no book the
/// caps pass — honest or not — can reach it; tripping it means a byte
/// source the caps never saw.
pub(super) const MAX_PERSISTED_BYTES: u64 = 128 * 1024 * 1024;

/// Meter charged immediately before every row insert in `commit_import`.
///
/// One instance per publication commit — never shared across a batch.
/// Charging *before* each write (not at the commit boundary) matters
/// because the WAL grows on disk while the transaction runs: a bomb must
/// abort within one row of the ceiling, not after gigabytes of transient
/// WAL. The `Err` it returns rides the existing failure path, which rolls
/// the transaction back and sweeps the staged book and cover.
pub(super) struct PersistBudget {
    rows: u64,
    bytes: u64,
    max_rows: u64,
    max_bytes: u64,
}

impl PersistBudget {
    /// The production meter, sized by the constants above.
    pub(super) fn for_import() -> Self {
        Self::with_limits(MAX_PERSISTED_ROWS, MAX_PERSISTED_BYTES)
    }

    /// A meter with explicit ceilings; tests use tiny ones to reach the
    /// trip path without building a multi-hundred-MB fixture.
    pub(super) fn with_limits(max_rows: u64, max_bytes: u64) -> Self {
        Self { rows: 0, bytes: 0, max_rows, max_bytes }
    }

    /// Charges one row carrying `bytes` of variable-length values.
    /// Returns `CoreError::InvalidPublication` naming the tripped limit
    /// and its value, so the failure is diagnosable from a log.
    pub(super) fn charge(&mut self, bytes: usize) -> Result<(), CoreError> {
        self.rows += 1;
        self.bytes = self.bytes.saturating_add(bytes as u64);
        if self.rows > self.max_rows {
            return Err(CoreError::InvalidPublication(format!(
                "import would persist more than {} rows for one publication",
                self.max_rows
            )));
        }
        if self.bytes > self.max_bytes {
            return Err(CoreError::InvalidPublication(format!(
                "import would persist more than {} bytes for one publication",
                self.max_bytes
            )));
        }
        Ok(())
    }
}
