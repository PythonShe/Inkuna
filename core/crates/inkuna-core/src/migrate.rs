//! Append-only versioned migrations tracked via SQLite's `user_version`
//! pragma. Never edit a shipped entry — add a new version. (refinery would
//! be preferred, but it currently caps rusqlite below our version; revisit.)
//!
//! v2 is not pure SQL: legacy v1 rows point at external absolute paths the
//! core never copied, so adoption does file-system work (copy in, hash,
//! relativize) inside the migration transaction. Files copied by a migration
//! that later rolls back are unreferenced and get swept at the next open.

use std::collections::HashSet;
use std::path::Path;

use rusqlite::{Connection, Transaction};

use crate::import::copy_and_hash;
use crate::CoreError;

pub(crate) const SCHEMA_VERSION: i64 = 2;

// 0001: initial schema (shipped — iOS opens this DB; never edit).
const V1_SQL: &str = "
CREATE TABLE publications (
    id          TEXT PRIMARY KEY,
    title       TEXT NOT NULL,
    authors     TEXT NOT NULL DEFAULT '',
    language    TEXT,
    format      TEXT NOT NULL,
    file_path   TEXT NOT NULL,
    added_at    INTEGER NOT NULL,
    progression REAL NOT NULL DEFAULT 0
);
";

// 0002: core-owned storage, reading state, sessions, bookmarks, settings.
// All file paths are stored relative to the data dir (iOS container paths
// change across installs); the FFI layer absolutizes on read.
const V2_SQL: &str = "
ALTER TABLE publications ADD COLUMN content_hash   TEXT;
ALTER TABLE publications ADD COLUMN cover_path     TEXT;
ALTER TABLE publications ADD COLUMN finished_at    INTEGER;
ALTER TABLE publications ADD COLUMN last_opened_at INTEGER;
ALTER TABLE publications ADD COLUMN locator        TEXT;
ALTER TABLE publications ADD COLUMN position_count INTEGER;

CREATE TABLE resources (
    id             TEXT PRIMARY KEY,
    publication_id TEXT NOT NULL REFERENCES publications(id) ON DELETE CASCADE,
    spine_idx      INTEGER NOT NULL,
    href           TEXT NOT NULL
);
CREATE INDEX idx_resources_publication ON resources(publication_id);

CREATE TABLE resource_text (
    resource_id TEXT PRIMARY KEY REFERENCES resources(id) ON DELETE CASCADE,
    body        TEXT NOT NULL
);

CREATE TABLE chapters (
    id             TEXT PRIMARY KEY,
    publication_id TEXT NOT NULL REFERENCES publications(id) ON DELETE CASCADE,
    idx            INTEGER NOT NULL,
    title          TEXT NOT NULL,
    href           TEXT NOT NULL,
    depth          INTEGER NOT NULL
);
CREATE INDEX idx_chapters_publication ON chapters(publication_id);

CREATE TABLE bookmarks (
    id             TEXT PRIMARY KEY,
    publication_id TEXT NOT NULL REFERENCES publications(id) ON DELETE CASCADE,
    locator        TEXT NOT NULL,
    progression    REAL NOT NULL,
    created_at     INTEGER NOT NULL
);
CREATE INDEX idx_bookmarks_publication ON bookmarks(publication_id);

CREATE TABLE sessions (
    id                TEXT PRIMARY KEY,
    publication_id    TEXT NOT NULL REFERENCES publications(id) ON DELETE CASCADE,
    started_at        INTEGER NOT NULL,
    ended_at          INTEGER,
    updated_at        INTEGER NOT NULL,
    start_progression REAL NOT NULL,
    end_progression   REAL NOT NULL,
    start_position    INTEGER,
    end_position      INTEGER
);
CREATE INDEX idx_sessions_publication ON sessions(publication_id);
CREATE INDEX idx_sessions_started ON sessions(started_at);

CREATE TABLE settings (
    id             INTEGER PRIMARY KEY CHECK (id = 1),
    onboarded      INTEGER NOT NULL,
    reading_theme  TEXT NOT NULL,
    text_size_step INTEGER NOT NULL,
    brightness     REAL NOT NULL
);
INSERT INTO settings (id, onboarded, reading_theme, text_size_step, brightness)
VALUES (1, 0, 'paper', 2, 0.78);
";

// Created after legacy adoption so duplicate legacy files are dropped as
// rows, never surfaced as a constraint failure mid-migration.
const V2_INDEX_SQL: &str = "
CREATE UNIQUE INDEX idx_publications_content_hash ON publications(content_hash);
";

pub(crate) fn migrate(conn: &mut Connection, data_dir: &Path) -> Result<(), CoreError> {
    loop {
        let version: i64 =
            conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version >= SCHEMA_VERSION {
            return Ok(());
        }
        let tx = conn.transaction()?;
        match version {
            0 => tx.execute_batch(V1_SQL)?,
            1 => {
                tx.execute_batch(V2_SQL)?;
                adopt_legacy_rows(&tx, data_dir)?;
                tx.execute_batch(V2_INDEX_SQL)?;
            }
            // The loop guard makes other values impossible.
            _ => return Ok(()),
        }
        tx.pragma_update(None, "user_version", version + 1)?;
        tx.commit()?;
    }
}

/// Adopts every v1 row into core-owned storage: copy the external file into
/// `books/<id>.<format>`, compute its BLAKE3 hash, rewrite `file_path`
/// relative. Rows whose source file is missing or unreadable — or whose
/// content duplicates an already-adopted row — are dropped. After this,
/// every surviving row is core-owned: `remove()` is always safe and dedupe
/// always has a hash.
fn adopt_legacy_rows(tx: &Transaction, data_dir: &Path) -> Result<(), CoreError> {
    let rows: Vec<(String, String, String)> = {
        let mut stmt = tx.prepare("SELECT id, file_path, format FROM publications")?;
        let mapped = stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
        mapped.collect::<Result<_, _>>()?
    };

    let mut seen = HashSet::new();
    for (id, file_path, format) in rows {
        let rel = format!("books/{id}.{format}");
        let dest = data_dir.join(&rel);
        match copy_and_hash(Path::new(&file_path), &dest) {
            Ok(hash) if seen.insert(hash.clone()) => {
                tx.execute(
                    "UPDATE publications SET file_path = ?1, content_hash = ?2 WHERE id = ?3",
                    rusqlite::params![rel, hash, id],
                )?;
            }
            // Byte-identical duplicate of an already-adopted row.
            Ok(_) => {
                let _ = std::fs::remove_file(&dest);
                tx.execute("DELETE FROM publications WHERE id = ?1", [&id])?;
            }
            // Source missing or unreadable.
            Err(_) => {
                let _ = std::fs::remove_file(&dest);
                tx.execute("DELETE FROM publications WHERE id = ?1", [&id])?;
            }
        }
    }
    Ok(())
}
