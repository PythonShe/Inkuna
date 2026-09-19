//! The library facade itself: opening a data dir and keeping its files and
//! rows in agreement.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::Connection;

use super::corpus::stored_corpus_digest;
use crate::CoreError;
use crate::core::db::{migrate, open_connection, ReaderPool, READER_POOL_SIZE};
use crate::core::time::unix_now;
use crate::features::search::SearchIndex;

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;

/// The data-dir subdirectory holding per-book extracted publisher-font
/// caches (`pubfonts/<publication-id>/<content-hash>.<ext>`). The FFI
/// builds session cache paths from it; [`Library::remove`] and the
/// open-time sweep delete a book's directory with the book.
pub const PUBLISHER_FONT_DIR: &str = "pubfonts";

/// The library facade: one SQLite DB plus core-owned book/cover storage
/// under a single data dir. One writer connection (mutations only, each in
/// a transaction; copying, hashing and parsing always happen outside the
/// lock) and a fixed reader pool so reads never queue behind an import.
///
/// The writer lock is also the ordering point for the files a publication
/// owns, because a restore adopts a removed book's id and therefore writes
/// to the very paths a removal deletes. Both sides settle that under the
/// lock, and only after claiming the row in the same transaction:
/// [`remove`](Library::remove) tombstones the live row and then unlinks,
/// and the import commit renames a restored book and cover into place only
/// once `revive` reported the tombstone claimed. The bounded file work
/// either side does under the lock (a few renames and unlinks) is the one
/// deliberate exception to "no I/O under the lock"; the rule is about the
/// copy/hash/parse, which still happens outside it.
pub struct Library {
    pub(crate) data_dir: PathBuf,
    pub(crate) writer: Mutex<Connection>,
    pub(crate) readers: ReaderPool,
    pub(crate) search: SearchIndex,
}

impl Library {
    /// Opens (creating if needed) the library rooted at `data_dir`:
    /// `inkuna.db`, `books/`, and `covers/` all live under it and are owned
    /// by the core. Runs pending migrations, then sweeps files unreferenced
    /// by any row (crash-recovery for interrupted imports).
    ///
    /// One `Library` per `data_dir` is a hard requirement: because the sweep
    /// cannot tell an abandoned staging file from a live one, opening a
    /// second concurrent instance on the same directory deletes the first
    /// one's in-flight import.
    pub fn open(data_dir: impl AsRef<Path>) -> Result<Library, CoreError> {
        let data_dir = data_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(data_dir.join("books"))?;
        std::fs::create_dir_all(data_dir.join("covers"))?;

        let db_path = data_dir.join("inkuna.db");
        let mut writer = open_connection(&db_path)?;
        migrate(&mut writer, &data_dir)?;
        let readers = ReaderPool::open(&db_path, READER_POOL_SIZE)?;
        let search = SearchIndex::open(&data_dir)?;
        // Heal the index against the database off the open path; a fresh
        // install and an unchanged library both make this a cheap no-op.
        // The V8 rebaseline chains FIRST on the same thread, so the
        // reconcile body never indexes a corpus the rebaseline is about
        // to replace; reads meanwhile see NULL coordinate columns and
        // fall back to the read-time default, so nothing waits on it.
        // The V12 edition backfill chains LAST, after the reconcile body,
        // on the same thread and the same cancel flag: it touches no
        // corpus, so it has no ordering constraint against the index —
        // and on a V11→V12 upgrade its pending set is the whole library
        // while the rebaseline's is empty, so running it ahead of the
        // reconcile would newly gate search behind a whole-library
        // zip-open and OPF parse. Ordering is pinned by
        // `the_post_pass_runs_after_the_reconcile_body`.
        let index_handle = search.write_handle();
        let rebaseline_data_dir = data_dir.clone();
        let rebaseline_db_path = db_path.clone();
        let backfill_data_dir = data_dir.clone();
        let backfill_db_path = db_path.clone();
        search.spawn_reconcile(
            db_path,
            move |cancel| {
                super::rebaseline::run(
                    &rebaseline_data_dir,
                    &rebaseline_db_path,
                    &index_handle,
                    cancel,
                );
            },
            move |cancel| {
                super::edition_backfill::run(&backfill_data_dir, &backfill_db_path, cancel);
            },
        );

        let library = Library {
            data_dir,
            writer: Mutex::new(writer),
            readers,
            search,
        };
        library.sweep()?;
        Ok(library)
    }

    /// The storage root this library was opened on. Every path a
    /// publication carries is relative to it — iOS container paths change
    /// across installs, so absolute paths are never persisted — which
    /// makes this the only correct base for absolutizing `file_path` and
    /// `cover_path` on the way out to a shell.
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Removes a book from the library: every byte it occupies goes — the
    /// book file, the cover, the extracted publisher-font cache, the
    /// search docs, and every purely derived row (`resources`, and with
    /// them `resource_text`, plus `chapters` and `resource_positions`,
    /// all of which a re-import rebuilds deterministically from the same
    /// bytes).
    ///
    /// What stays is the reading history: the publication row itself is
    /// tombstoned rather than deleted (`removed_at` stamped, `file_path`
    /// blanked, `cover_path` dropped), so the `sessions` and `bookmarks`
    /// rows hanging off it — and its progression, position, and finished
    /// state — survive untouched, with no data movement at all. Importing
    /// the same bytes again matches the tombstone on `content_hash` and
    /// lights all of it back up (see the import pipeline's restore path).
    /// A tombstone is invisible to every library read, so from a shell's
    /// point of view the book is gone.
    ///
    /// Nothing is read before the writer transaction opens: the paths to
    /// unlink are read *through* it, and the row is only deleted from
    /// after the tombstone `UPDATE` reports it claimed. A removed (or
    /// concurrently removing) book is therefore `NotFound`, and two
    /// removes of one book can never both reach the unlinks — which
    /// matters because those paths are exactly the ones a restore of the
    /// same content adopts.
    ///
    /// File deletion is idempotent — missing files are not an error — and
    /// always confined to the data dir because DB paths are relative by
    /// construction (the font cache path is built from the id here).
    pub fn remove(&self, id: &str) -> Result<(), CoreError> {
        {
            let mut conn = self.writer.lock().unwrap();
            // Immediate: the row this reads is the row it is about to
            // claim, so the write lock is taken up front rather than
            // upgraded from under a snapshot read.
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;

            // Read through the transaction, never off the reader pool: a
            // pre-lock read can go stale between the read and the claim,
            // and these paths drive irreversible unlinks.
            let paths = {
                let mut stmt = tx.prepare(
                    "SELECT file_path, cover_path FROM publications_all
                      WHERE id = ?1 AND removed_at IS NULL",
                )?;
                let mut rows = stmt.query_map([id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
                })?;
                rows.next().transpose()?
            };
            let Some((file_path, cover_path)) = paths else {
                return Err(CoreError::NotFound(id.to_string()));
            };

            // Digested BEFORE the derived rows go: `resource_text` IS the
            // corpus these coordinates index, and in a moment it will not
            // exist. Restore digests the corpus it rebuilds and compares,
            // so a converter or projection change between now and then
            // degrades the coordinates instead of pointing them at
            // different words. A row with no resources digests to NULL,
            // which restore treats exactly like a mismatch.
            let corpus_digest = stored_corpus_digest(&tx, id)?;
            // `file_path` is NOT NULL, so "no file" is `''`.
            // `reconciled_at` is cleared as a fail-safe: should any read
            // ever reach a tombstone, it must not be told its (now
            // deleted) corpus is canonical.
            let claimed = tx.execute(
                "UPDATE publications_all
                    SET removed_at = ?1, corpus_digest = ?2, file_path = '',
                        cover_path = NULL, reconciled_at = NULL
                  WHERE id = ?3 AND removed_at IS NULL",
                rusqlite::params![unix_now(), corpus_digest, id],
            )?;
            if claimed != 1 {
                // Unreachable: the row was read through this very
                // transaction. Bail rather than delete the files of a row
                // this call does not own.
                return Err(CoreError::NotFound(id.to_string()));
            }
            // The row stays, so these no longer cascade: drop them by hand.
            // `resource_text` still cascades from `resources`.
            for table in ["resources", "chapters", "resource_positions"] {
                tx.execute(
                    &format!("DELETE FROM {table} WHERE publication_id = ?1"),
                    [id],
                )?;
            }
            tx.commit()?;

            // Still holding the writer lock, and deliberately so. A restore
            // of the same content claims this very id and writes its book
            // and cover to these exact paths. It renames them into place
            // under this same lock, after its `revive` claimed the row, so
            // holding the lock across these unlinks is what keeps them
            // strictly before any restore's placement — and the claim
            // above is what keeps a second remove from repeating them
            // against files that restore has since put back. If the
            // unlinks ran after the lock was released, a restore that
            // revived in between would have its files deleted out from
            // under a live row, and nothing heals that (the sweep only
            // removes *unreferenced* files). Three unlinks and one small
            // directory removal is bounded, non-blocking work — not the
            // copy/hash/parse the "no I/O under the lock" rule is about.
            let _ = std::fs::remove_file(self.data_dir.join(&file_path));
            if let Some(cover) = &cover_path {
                let _ = std::fs::remove_file(self.data_dir.join(cover));
            }
            let _ = std::fs::remove_dir_all(self.data_dir.join(PUBLISHER_FONT_DIR).join(id));
        }
        // Derived data: a failure here only leaves stale docs that the
        // next open's reconcile drops, so the remove still succeeds.
        if let Err(e) = self.search.delete_publication(id) {
            log::warn!("search index delete failed for {id}: {e}");
        }
        Ok(())
    }

    /// Deletes files under `books/` and `covers/` that no *live*
    /// publication row references — leftovers of imports interrupted
    /// between the file rename and the DB commit — plus stray `.tmp`
    /// staging files.
    ///
    /// Tombstoned rows are excluded from both referenced sets on purpose.
    /// A tombstone's blanked `file_path` could not shield a file anyway,
    /// but its id must not shield a publisher-font directory: a remove
    /// interrupted before the `remove_dir_all` has to be finishable here.
    fn sweep(&self) -> Result<(), CoreError> {
        let referenced: HashSet<String> = self.readers.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT file_path, cover_path FROM publications_all WHERE removed_at IS NULL",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
            })?;
            let mut set = HashSet::new();
            for row in rows {
                let (file_path, cover_path) = row?;
                set.insert(file_path);
                if let Some(cover) = cover_path {
                    set.insert(cover);
                }
            }
            Ok(set)
        })?;

        for sub in ["books", "covers"] {
            for entry in std::fs::read_dir(self.data_dir.join(sub))? {
                let entry = entry?;
                if !entry.file_type()?.is_file() {
                    continue;
                }
                let rel = format!("{sub}/{}", entry.file_name().to_string_lossy());
                if !referenced.contains(&rel) {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }

        // Publisher-font caches are keyed by publication id; a directory
        // whose id has no row is a leftover of an interrupted delete.
        // The dir is optional (created lazily at first reader open).
        let ids: HashSet<String> = self.readers.with(|conn| {
            let mut stmt =
                conn.prepare("SELECT id FROM publications_all WHERE removed_at IS NULL")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            let mut set = HashSet::new();
            for row in rows {
                set.insert(row?);
            }
            Ok(set)
        })?;
        if let Ok(entries) = std::fs::read_dir(self.data_dir.join(PUBLISHER_FONT_DIR)) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if !ids.contains(&name) {
                    let _ = std::fs::remove_dir_all(entry.path());
                }
            }
        }
        Ok(())
    }
}
