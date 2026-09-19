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
/// a transaction; file I/O and parsing always happen outside the lock —
/// [`remove`](Library::remove)'s unlinks are the one deliberate exception,
/// and it says why) and a fixed reader pool so reads never queue behind an
/// import.
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
        let index_handle = search.write_handle();
        let rebaseline_data_dir = data_dir.clone();
        let rebaseline_db_path = db_path.clone();
        search.spawn_reconcile(db_path, move |cancel| {
            super::rebaseline::run(&rebaseline_data_dir, &rebaseline_db_path, &index_handle, cancel);
        });

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
    /// File deletion is idempotent — missing files are not an error — and
    /// always confined to the data dir because DB paths are relative by
    /// construction (the font cache path is built from the id here).
    /// Removing an already-removed book is `NotFound`, as before: the
    /// lookup below cannot see a tombstone.
    pub fn remove(&self, id: &str) -> Result<(), CoreError> {
        let publication = self.publication(id)?;
        {
            let mut conn = self.writer.lock().unwrap();
            let tx = conn.transaction()?;
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
            tx.execute(
                "UPDATE publications
                    SET removed_at = ?1, corpus_digest = ?2, file_path = '',
                        cover_path = NULL, reconciled_at = NULL
                  WHERE id = ?3 AND removed_at IS NULL",
                rusqlite::params![unix_now(), corpus_digest, id],
            )?;
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
            // and cover to these exact paths; if the unlinks ran after the
            // lock was released, a restore that revived the row in between
            // would have its files deleted out from under a live row, and
            // nothing heals that (the sweep only removes *unreferenced*
            // files). Reviving a tombstone needs this lock, so finishing
            // the deletes under it orders them strictly before any restore.
            // Three unlinks and one small directory removal is bounded,
            // non-blocking work — not the copy/hash/parse the "no I/O under
            // the lock" rule is about.
            let _ = std::fs::remove_file(self.data_dir.join(&publication.file_path));
            if let Some(cover) = &publication.cover_path {
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
                "SELECT file_path, cover_path FROM publications WHERE removed_at IS NULL",
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
            let mut stmt = conn.prepare("SELECT id FROM publications WHERE removed_at IS NULL")?;
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
