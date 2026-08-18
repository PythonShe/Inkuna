//! The library facade itself: opening a data dir and keeping its files and
//! rows in agreement.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::Connection;

use crate::core::db::{migrate, open_connection, ReaderPool, READER_POOL_SIZE};
use crate::features::search::SearchIndex;
use crate::CoreError;

/// The library facade: one SQLite DB plus core-owned book/cover storage
/// under a single data dir. One writer connection (mutations only, each in
/// a transaction; file I/O and parsing always happen outside the lock) and
/// a fixed reader pool so reads never queue behind an import.
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
        search.spawn_reconcile(db_path);

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

    /// Removes the publication row (child tables cascade), its book file,
    /// and its cover. File deletion is idempotent — missing files are not
    /// an error — and always confined to the data dir because DB paths are
    /// relative by construction.
    pub fn remove(&self, id: &str) -> Result<(), CoreError> {
        let publication = self.publication(id)?;
        {
            let conn = self.writer.lock().unwrap();
            conn.execute("DELETE FROM publications WHERE id = ?1", [id])?;
        }
        let _ = std::fs::remove_file(self.data_dir.join(&publication.file_path));
        if let Some(cover) = &publication.cover_path {
            let _ = std::fs::remove_file(self.data_dir.join(cover));
        }
        // Derived data: a failure here only leaves stale docs that the
        // next open's reconcile drops, so the remove still succeeds.
        if let Err(e) = self.search.delete_publication(id) {
            log::warn!("search index delete failed for {id}: {e}");
        }
        Ok(())
    }

    /// Deletes files under `books/` and `covers/` that no publication row
    /// references — leftovers of imports interrupted between the file
    /// rename and the DB commit — plus stray `.tmp` staging files.
    fn sweep(&self) -> Result<(), CoreError> {
        let referenced: HashSet<String> = self.readers.with(|conn| {
            let mut stmt = conn.prepare("SELECT file_path, cover_path FROM publications")?;
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
        Ok(())
    }
}
