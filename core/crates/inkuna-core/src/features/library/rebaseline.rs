//! The V8 rebaseline: a background pass that moves every pre-engine book
//! onto the canonical projection — corpus replaced with `extract_corpus`
//! output, synthetic positions recomputed, and legacy Readium locator
//! JSON converted to content coordinates — one book per transaction,
//! most recently read books first. `reconciled_at` gates the pass:
//! idempotent, crash-resumable, and skipped entirely for books imported
//! after the engine swap (import stamps them itself).
//!
//! Runs on the search reconcile thread, *before* the search reconcile
//! body, so the index never indexes a corpus this pass is about to
//! replace; each book is re-indexed right after its commit, keeping
//! search consistent book-by-book while the pass runs.

use std::path::Path;

use inkuna_content::{resolve_href, split_fragment, MAX_TOTAL_TEXT_BYTES};
use inkuna_engine::extract_corpus;
use rusqlite::{Connection, TransactionBehavior};

use crate::core::db::open_connection;
use crate::core::time::unix_now;
use crate::features::progress::synthetic_positions;
use crate::features::search::IndexWriteHandle;
use crate::CoreError;

#[cfg(test)]
#[path = "rebaseline_tests.rs"]
mod tests;

/// One spine resource as the pass reads it: `(resource_id, spine_idx,
/// href)`.
type ResourceRow = (String, u32, String);

/// Runs the whole pass; a failure opening the connection logs and
/// returns. Opens its OWN writer connection (WAL + busy_timeout make a
/// second writer safe; per-book transactions are short), so `Library`
/// reads and writes proceed while it runs — coordinate columns still
/// NULL simply surface the read-time default until their book is done.
pub(crate) fn run(data_dir: &Path, db_path: &Path, index: &IndexWriteHandle) {
    if let Err(e) = run_with_hook(data_dir, db_path, index, |_| Ok(())) {
        log::warn!("v8 rebaseline could not run: {e}");
    }
}

/// The pass body with a per-book fault-injection hook (`|_| Ok(())` in
/// production): the hook runs inside each book's transaction, so a hook
/// error exercises the roll-back-and-continue path.
fn run_with_hook(
    data_dir: &Path,
    db_path: &Path,
    index: &IndexWriteHandle,
    mut hook: impl FnMut(&str) -> Result<(), CoreError>,
) -> Result<(), CoreError> {
    let mut conn = open_connection(db_path)?;
    let pending: Vec<(String, String)> = {
        let mut stmt = conn.prepare(
            "SELECT id, file_path FROM publications
             WHERE reconciled_at IS NULL
             ORDER BY last_opened_at DESC NULLS LAST",
        )?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect::<Result<_, _>>()?
    };
    for (id, file_path) in pending {
        // Per-book fault isolation: an error rolls back that book's
        // transaction alone; the book retries at the next open because
        // `reconciled_at` was never stamped.
        match rebaseline_book(&mut conn, data_dir, &id, &file_path, &mut hook) {
            Ok(()) => reindex_book(&conn, index, &id),
            Err(e) => log::warn!("rebaseline of {id} failed (retries next open): {e}"),
        }
    }
    Ok(())
}

/// One book, all inside one IMMEDIATE transaction — a crash retries the
/// whole book at the next open.
fn rebaseline_book(
    conn: &mut Connection,
    data_dir: &Path,
    id: &str,
    file_path: &str,
    hook: &mut impl FnMut(&str) -> Result<(), CoreError>,
) -> Result<(), CoreError> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    hook(id)?;

    let resources: Vec<ResourceRow> = {
        let mut stmt = tx.prepare_cached(
            "SELECT id, spine_idx, href FROM resources
             WHERE publication_id = ?1 ORDER BY spine_idx",
        )?;
        let rows = stmt.query_map([id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
        rows.collect::<Result<_, _>>()?
    };

    // Steps 1–2 need the book's bytes. A missing/unreadable file still
    // reconciles steps 3–5 with defaults — char lengths are unknowable
    // then, so the coordinate falls back to the chapter start when the
    // href resolves, else (0, 0) — keeps whatever `resource_text` it
    // had, and is stamped: the book still opens; its position falls back
    // at read time.
    let file = data_dir.join(file_path);
    let char_lens: Vec<Option<u64>> = if file.is_file() {
        let hrefs: Vec<String> = resources.iter().map(|(_, _, href)| href.clone()).collect();
        let corpus = extract_corpus(&file, &hrefs, MAX_TOTAL_TEXT_BYTES);

        // 1. The corpus becomes the canonical projection.
        for ((resource_id, _, _), text) in resources.iter().zip(&corpus) {
            match text {
                Some(body) => tx.execute(
                    "INSERT OR REPLACE INTO resource_text (resource_id, body) VALUES (?1, ?2)",
                    rusqlite::params![resource_id, body],
                )?,
                None => tx.execute(
                    "DELETE FROM resource_text WHERE resource_id = ?1",
                    [resource_id],
                )?,
            };
        }

        // 2. Synthetic positions over the new corpus.
        let counts: Vec<u64> = corpus
            .iter()
            .map(|text| {
                text.as_deref()
                    .map_or(0, |body| body.chars().count() as u64)
            })
            .collect();
        tx.execute(
            "DELETE FROM resource_positions WHERE publication_id = ?1",
            [id],
        )?;
        let rows = synthetic_positions(&counts);
        let total: u32 = rows
            .iter()
            .fold(0u32, |sum, &(_, _, count)| sum.saturating_add(count));
        {
            let mut insert = tx.prepare_cached(
                "INSERT INTO resource_positions
                    (publication_id, spine_idx, start_position, position_count)
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            for &(spine_idx, start, count) in &rows {
                insert.execute(rusqlite::params![id, spine_idx, start, count])?;
            }
        }
        if total > 0 {
            tx.execute(
                "UPDATE publications SET position_count = ?1 WHERE id = ?2",
                rusqlite::params![total, id],
            )?;
        }
        counts.into_iter().map(Some).collect()
    } else {
        log::warn!(
            "rebaseline of {id}: book file missing at {}; converting locators with defaults",
            file.display()
        );
        vec![None; resources.len()]
    };

    // 3. The publication's Readium locator becomes a content coordinate.
    let locator: Option<String> = tx.query_row(
        "SELECT locator FROM publications WHERE id = ?1",
        [id],
        |row| row.get(0),
    )?;
    if let Some(locator) = locator {
        let (spine_idx, char_offset) = convert_locator(&locator, &resources, &char_lens);
        tx.execute(
            "UPDATE publications
             SET position_spine_idx = ?1, position_char_offset = ?2, locator = NULL
             WHERE id = ?3",
            rusqlite::params![spine_idx, char_offset as i64, id],
        )?;
    }

    // 4. Every bookmark, same conversion; the column is NOT NULL, so a
    // consumed locator becomes ''.
    let bookmarks: Vec<(String, String)> = {
        let mut stmt =
            tx.prepare_cached("SELECT id, locator FROM bookmarks WHERE publication_id = ?1")?;
        let rows = stmt.query_map([id], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect::<Result<_, _>>()?
    };
    for (bookmark_id, locator) in bookmarks {
        let (spine_idx, char_offset) = convert_locator(&locator, &resources, &char_lens);
        tx.execute(
            "UPDATE bookmarks
             SET position_spine_idx = ?1, position_char_offset = ?2, locator = ''
             WHERE id = ?3",
            rusqlite::params![spine_idx, char_offset as i64, bookmark_id],
        )?;
    }

    // 5. Stamp: the gate that makes the pass idempotent.
    tx.execute(
        "UPDATE publications SET reconciled_at = ?1 WHERE id = ?2",
        rusqlite::params![unix_now(), id],
    )?;
    tx.commit()?;
    Ok(())
}

/// Legacy Readium locator JSON → content coordinate, never failing:
/// every failure shape has a written default. Unparseable JSON or an
/// unresolvable href → `(0, 0)`; a resolvable href with bad/absent
/// progression (or an unknowable char length) → the chapter start
/// `(spine_idx, 0)`.
fn convert_locator(
    locator: &str,
    resources: &[ResourceRow],
    char_lens: &[Option<u64>],
) -> (u32, u64) {
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(locator) else {
        return (0, 0);
    };
    let Some(href) = parsed.get("href").and_then(|v| v.as_str()) else {
        return (0, 0);
    };
    let normalized = resolve_href("", split_fragment(href).0);
    let Some(at) = resources
        .iter()
        .position(|(_, _, resource_href)| *resource_href == normalized)
    else {
        return (0, 0);
    };
    let spine_idx = resources[at].1;
    let progression = parsed
        .get("locations")
        .and_then(|l| l.get("progression"))
        .and_then(|p| p.as_f64())
        .filter(|p| p.is_finite());
    let Some(progression) = progression else {
        return (spine_idx, 0);
    };
    let Some(len) = char_lens.get(at).copied().flatten() else {
        return (spine_idx, 0);
    };
    let offset = (progression.clamp(0.0, 1.0) * len as f64) as u64;
    (spine_idx, offset.min(len.saturating_sub(1)))
}

/// Re-indexes one just-committed book so library-wide search stays
/// consistent with its new corpus while the pass runs. Derived data: a
/// failure only logs — the trailing search reconcile or the next open
/// heals it.
fn reindex_book(conn: &Connection, index: &IndexWriteHandle, id: &str) {
    let rows: Result<Vec<(u32, String)>, CoreError> = (|| {
        let mut stmt = conn.prepare_cached(
            "SELECT r.spine_idx, t.body FROM resources r
             JOIN resource_text t ON t.resource_id = r.id
             WHERE r.publication_id = ?1 ORDER BY r.spine_idx",
        )?;
        let rows = stmt.query_map([id], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.collect::<Result<_, rusqlite::Error>>()?)
    })();
    let outcome = rows.and_then(|rows| {
        index.index_publication(id, rows.iter().map(|(idx, body)| (*idx, body.as_str())))
    });
    if let Err(e) = outcome {
        log::warn!("post-rebaseline reindex of {id} failed: {e}");
    }
}
