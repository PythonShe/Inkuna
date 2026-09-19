//! The V12 edition backfill: a background pass that fills `edition_key`
//! for every book imported before the column existed, one book per
//! transaction, most recently read books first. `edition_scanned_at` gates
//! it — idempotent, crash-resumable, and skipped entirely for books
//! imported after V12 (import stamps them itself).
//!
//! The migration cannot do this work: the key comes out of the book's OPF,
//! and re-opening every archive in a library is not something a migration
//! may do on the open path. Leaving the column NULL forever was the other
//! option and is worse than it looks — every library already installed
//! would keep counting a finished book and a post-V12 re-import of the
//! same edition as two, which is exactly the case the column was added
//! for.
//!
//! Runs on the search reconcile thread, chained *after* the V8 rebaseline,
//! because these writes touch no corpus and nothing downstream waits on
//! them; a book whose key is not filled yet simply counts as itself.
//!
//! Tombstones are never scanned: their file is gone, so there is nothing
//! to read, and V11 freezes them against writes anyway. They keep
//! `edition_key` NULL and count as themselves — the safe direction.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::{Connection, OptionalExtension, TransactionBehavior};

use super::edition::edition_key;
use crate::CoreError;
use crate::core::db::open_connection;
use crate::core::time::unix_now;
use crate::formats::epub;

#[cfg(test)]
#[path = "edition_backfill_tests.rs"]
mod tests;

/// Runs the whole pass; a failure opening the connection logs and
/// returns. Opens its OWN writer connection, exactly as the rebaseline
/// does — WAL plus `busy_timeout` make a second writer safe, and each
/// book's transaction is one small UPDATE.
///
/// `cancel` is the `SearchIndex`'s drop-time flag: a dropped `Library`
/// sets it before joining the reconcile thread, and the pass bails at the
/// next check, leaving the remaining books unstamped for the next open.
pub(crate) fn run(data_dir: &Path, db_path: &Path, cancel: &AtomicBool) {
    if let Err(e) = run_pass(data_dir, db_path, cancel) {
        log::warn!("v12 edition backfill could not run: {e}");
    }
}

fn run_pass(data_dir: &Path, db_path: &Path, cancel: &AtomicBool) -> Result<(), CoreError> {
    let mut conn = open_connection(db_path)?;
    let pending: Vec<(String, String)> = {
        let mut stmt = conn.prepare(
            // A tombstone has no file to read an identifier out of, so it
            // is excluded here rather than stamped — it would otherwise be
            // retried on every open, forever.
            "SELECT id, file_path FROM publications
             WHERE edition_scanned_at IS NULL AND removed_at IS NULL
             ORDER BY last_opened_at DESC NULLS LAST",
        )?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect::<Result<_, _>>()?
    };
    for (id, file_path) in pending {
        // Checked between books: whatever is not yet stamped resumes at
        // the next open.
        if cancel.load(Ordering::Relaxed) {
            log::info!("v12 edition backfill cancelled; remaining books resume at the next open");
            return Ok(());
        }
        // Per-book fault isolation: an error rolls back that book's
        // transaction alone, and the book retries at the next open because
        // `edition_scanned_at` was never stamped.
        if let Err(e) = backfill_book(&mut conn, data_dir, &id, &file_path) {
            log::warn!("edition backfill of {id} failed (retries next open): {e}");
        }
    }
    Ok(())
}

/// One book: read its OPF metadata first, OUTSIDE any transaction, then
/// one short IMMEDIATE transaction for the write. Holding the write lock
/// across an archive open would starve other writers against the 5s
/// `busy_timeout` for no reason.
///
/// An unreadable, missing, or identifier-less file is NOT an error: it
/// records no identity and is stamped all the same. Without the stamp the
/// same unreadable file would be re-opened on every launch for the life of
/// the install.
fn backfill_book(
    conn: &mut Connection,
    data_dir: &Path,
    id: &str,
    file_path: &str,
) -> Result<(), CoreError> {
    let file = data_dir.join(file_path);
    let key = match epub::read_metadata(&file) {
        Ok(metadata) => metadata.unique_identifier.as_deref().and_then(edition_key),
        Err(e) => {
            log::warn!(
                "edition backfill of {id}: {} could not be read ({e}); recording no identity",
                file.display()
            );
            None
        }
    };

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    // Liveness, rechecked under the write lock: `removed_at IS NULL` held
    // when the pending snapshot was taken, but a `remove` since then has
    // left the row a tombstone. V11's freeze trigger would silently drop
    // the write anyway; checking explicitly is what keeps the stamp from
    // appearing to have been written when it was not. A vanished row
    // counts as removed for the same reason.
    let live = tx
        .query_row(
            "SELECT removed_at FROM publications WHERE id = ?1",
            [id],
            |row| row.get::<_, Option<i64>>(0),
        )
        .optional()?
        .is_some_and(|removed_at| removed_at.is_none());
    if !live {
        drop(tx);
        log::info!("edition backfill of {id} skipped: the book was removed mid-pass");
        return Ok(());
    }

    tx.execute(
        "UPDATE publications
            SET edition_key = ?1, edition_scanned_at = ?2
          WHERE id = ?3 AND removed_at IS NULL",
        rusqlite::params![key, unix_now(), id],
    )?;
    tx.commit()?;
    Ok(())
}
