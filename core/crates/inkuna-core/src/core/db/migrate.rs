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

use crate::core::files::copy_and_hash_unbounded;
use crate::features::library::title_key;
use crate::CoreError;

pub(crate) const SCHEMA_VERSION: i64 = 12;

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

// 0003: per-resource synthetic position ranges, reported by the shell once
// its navigator computes synthetic positions (the core never invents page
// numbers). `start_position` is 1-based and cumulative across the spine;
// chapter ranges are derived from these rows at query time.
const V3_SQL: &str = "
CREATE TABLE resource_positions (
    publication_id TEXT NOT NULL REFERENCES publications(id) ON DELETE CASCADE,
    spine_idx      INTEGER NOT NULL,
    start_position INTEGER NOT NULL,
    position_count INTEGER NOT NULL,
    PRIMARY KEY (publication_id, spine_idx)
);
";

// 0004: local account profile and the evening-reminder preference. The
// account is purely on-device (no server); empty strings mean "not set"
// and shells render their own placeholder.
const V4_SQL: &str = "
ALTER TABLE settings ADD COLUMN evening_reminder INTEGER NOT NULL DEFAULT 0;
ALTER TABLE settings ADD COLUMN account_name     TEXT NOT NULL DEFAULT '';
ALTER TABLE settings ADD COLUMN account_email    TEXT NOT NULL DEFAULT '';
";

// 0005: source charset retained for normalized plain-text imports. Native
// EPUBs and formats without a meaningful source charset keep NULL.
const V5_SQL: &str = "
ALTER TABLE publications ADD COLUMN text_encoding TEXT;
";

// 0006: configurable evening-reminder time, in minutes after local
// midnight. 1260 (21:00) is the fixed hour the shells shipped with, so
// existing installs keep firing when they always did.
const V6_SQL: &str = "
ALTER TABLE settings ADD COLUMN reminder_minutes INTEGER NOT NULL DEFAULT 1260;
";

// 0007: reader typography and layout. `reading_font` is opaque like
// `reading_theme` (shells own the roster; 'publisher' means the EPUB's own
// faces). Spacings are CSS-semantic — a unitless line-height multiplier and
// em offsets — and `reading_margins` is horizontal page padding in CSS px
// inside the rendering web view, so one stored value means the same thing
// on both shells.
const V7_SQL: &str = "
ALTER TABLE settings ADD COLUMN reading_font    TEXT    NOT NULL DEFAULT 'publisher';
ALTER TABLE settings ADD COLUMN reading_bold    INTEGER NOT NULL DEFAULT 0;
ALTER TABLE settings ADD COLUMN line_spacing    REAL    NOT NULL DEFAULT 1.65;
ALTER TABLE settings ADD COLUMN letter_spacing  REAL    NOT NULL DEFAULT 0;
ALTER TABLE settings ADD COLUMN word_spacing    REAL    NOT NULL DEFAULT 0;
ALTER TABLE settings ADD COLUMN reading_margins INTEGER NOT NULL DEFAULT 26;
";

// 0008: content coordinates (engine swap). position_spine_idx /
// position_char_offset are the canonical-projection coordinate
// replacing legacy locator JSON; `locator` columns are retained
// as-is until the per-book reconcile pass consumes them (publications
// NULLed / bookmarks set to '' after conversion). reconciled_at
// stamps a book whose corpus, synthetic positions, and locators have
// been rebaselined. Settings-units note (V7 precedent):
// reading_margins is reinterpreted from CSS px inside the rendering
// web view to engine layout points — numerically identical at 1x,
// so stored values carry over unchanged.
const V8_SQL: &str = "
ALTER TABLE publications ADD COLUMN position_spine_idx   INTEGER;
ALTER TABLE publications ADD COLUMN position_char_offset INTEGER;
ALTER TABLE publications ADD COLUMN reconciled_at        INTEGER;
ALTER TABLE bookmarks    ADD COLUMN position_spine_idx   INTEGER;
ALTER TABLE bookmarks    ADD COLUMN position_char_offset INTEGER;
";

// 0009: app-wide haptic feedback preference. On by default — every install
// so far has had haptics — and firing them stays shell work; the core only
// remembers the choice.
const V9_SQL: &str = "
ALTER TABLE settings ADD COLUMN haptics INTEGER NOT NULL DEFAULT 1;
";

// 0010: library view mode. Off (list) by default — the list is what every
// install so far has rendered — and laying out the grid stays shell work;
// the core only remembers the choice.
const V10_SQL: &str = "
ALTER TABLE settings ADD COLUMN library_grid INTEGER NOT NULL DEFAULT 0;
";

// 0011: tombstones. Removing a book frees its disk (file, cover, publisher
// fonts, search docs) and drops every purely derived row — but keeps the
// publication row itself, so the reading history hanging off it (sessions,
// bookmarks, progression, finished state) survives and reattaches when the
// same bytes come back. `removed_at` NULL means live; every read that means
// "a book in the library" filters on it. The tombstone keeps its
// `content_hash`, which is what re-import matches on.
//
// `corpus_digest` is what makes the reattachment safe. Every stored
// coordinate is a `(spine_idx, char_offset)` index into the canonical text
// projection, and `content_hash` identifies the *pre-conversion* source
// bytes — so a re-imported MOBI/AZW3/TXT is re-converted and re-projected
// by whatever build is running now, which may not produce the character
// stream the coordinates were taken against. Removal therefore stamps a
// digest of the corpus it is about to delete; restore digests the corpus it
// just built and reattaches coordinates only on an exact match. NULL means
// unknown provenance, which degrades exactly like a mismatch.
//
// In-table rather than a shelved copy on purpose: the stats overview
// aggregates `sessions` with no join to `publications`, so moving session
// rows anywhere would retroactively erase reading time from the weekly and
// monthly figures.
//
// There is no downgrade gate, and v10 builds are already out in the world:
// one of them opens this database and runs its own SQL against it. It knows
// nothing about `removed_at`, so every statement it issues against
// `publications` is written as if tombstones did not exist. Rather than
// teach the old binary a filter it cannot learn, v11 makes the name it
// queries mean what it always meant: the real table becomes
// `publications_all` and `publications` becomes a v10-shaped view of the
// live rows. A v10 build on a v11 database then behaves indistinguishably
// from a v10 build on a v10 database — no ghost rows on the All shelf, no
// unopenable entries, no "already in your library" for a file it cannot
// see. Views and triggers are part of the schema, so they bind whichever
// build opened the file.
//
// Every v11+ read and write names `publications_all` directly; the view
// exists solely for the old binary. It exposes exactly the v10 columns, in
// v10 order, so later versions may add columns to the base table without
// widening what v10 sees — and `rowid` explicitly, because v10's shelf
// ordering breaks ties on it.
const V11_SQL: &str = "
ALTER TABLE publications ADD COLUMN removed_at    INTEGER;
ALTER TABLE publications ADD COLUMN corpus_digest TEXT;

-- RENAME TO rewrites the referencing FK clauses in `sessions`, `resources`,
-- `chapters`, `bookmarks` and `resource_positions` and carries every index
-- over. Everything below is created *after* it, naming `publications_all`
-- outright, rather than created first and left to follow the rename — the
-- schema then says what it means without depending on how RENAME rewrites
-- a trigger body.
ALTER TABLE publications RENAME TO publications_all;

CREATE VIEW publications AS
  SELECT rowid AS rowid,
         id, title, authors, language, format, file_path, added_at,
         progression, content_hash, cover_path, finished_at, last_opened_at,
         locator, position_count, text_encoding, position_spine_idx,
         position_char_offset, reconciled_at
    FROM publications_all
   WHERE removed_at IS NULL;

-- A v10 `DELETE FROM publications WHERE id = ?` becomes the tombstone that
-- v11 would have written, minus the `corpus_digest` it cannot know — NULL,
-- which restore already treats as unknown provenance and degrades like a
-- mismatch. The derived rows go by hand exactly as `remove` drops them
-- (`resource_text` still cascades from `resources`), and RAISE(IGNORE)
-- then cancels the delete itself, silently, so the old binary sees the
-- success it expects.
CREATE TRIGGER publications_soft_delete
BEFORE DELETE ON publications_all
WHEN OLD.removed_at IS NULL
BEGIN
  DELETE FROM resources          WHERE publication_id = OLD.id;
  DELETE FROM chapters           WHERE publication_id = OLD.id;
  DELETE FROM resource_positions WHERE publication_id = OLD.id;
  UPDATE publications_all
     SET removed_at    = CAST(strftime('%s','now') AS INTEGER),
         file_path     = '',
         cover_path    = NULL,
         corpus_digest = NULL,
         reconciled_at = NULL
   WHERE id = OLD.id;
  SELECT RAISE(IGNORE);
END;

-- A tombstone is frozen: any write that would leave it a tombstone is
-- dropped on the floor. The view already keeps v10 away from tombstones
-- entirely, so this now guards the v11+ side alone — a rebaseline or
-- backfill pass that races a `remove` and reaches `publications_all`
-- directly. Restore is the sole exception and identifies itself by
-- clearing `removed_at` in the same statement.
CREATE TRIGGER publications_freeze_tombstone
BEFORE UPDATE ON publications_all
WHEN OLD.removed_at IS NOT NULL AND NEW.removed_at IS NOT NULL
BEGIN SELECT RAISE(IGNORE); END;

-- A view is only writable through INSTEAD OF triggers, and v10 writes to
-- this name. Each one forwards to the base table and then lets the
-- triggers above have the last word: the delete below fires
-- `publications_soft_delete`, whose RAISE(IGNORE) abandons the inner
-- statement alone and lets this trigger program continue, so a v10 remove
-- still lands as a tombstone.
CREATE TRIGGER publications_view_delete
INSTEAD OF DELETE ON publications
BEGIN
  DELETE FROM publications_all WHERE id = OLD.id;
END;

-- Writes back exactly the v10 columns; anything v11+ added keeps its
-- stored value, because the old binary has no opinion about it. The view
-- only ever yields live rows, so OLD.removed_at is NULL here and the
-- freeze trigger never fires.
CREATE TRIGGER publications_view_update
INSTEAD OF UPDATE ON publications
BEGIN
  UPDATE publications_all
     SET id                   = NEW.id,
         title                = NEW.title,
         authors              = NEW.authors,
         language             = NEW.language,
         format               = NEW.format,
         file_path            = NEW.file_path,
         added_at             = NEW.added_at,
         progression          = NEW.progression,
         content_hash         = NEW.content_hash,
         cover_path           = NEW.cover_path,
         finished_at          = NEW.finished_at,
         last_opened_at       = NEW.last_opened_at,
         locator              = NEW.locator,
         position_count       = NEW.position_count,
         text_encoding        = NEW.text_encoding,
         position_spine_idx   = NEW.position_spine_idx,
         position_char_offset = NEW.position_char_offset,
         reconciled_at        = NEW.reconciled_at
   WHERE id = OLD.id;
END;

-- The one place the view alone is not enough. v10's dedupe reads through
-- it, so a tombstone holding this content is invisible and the import
-- proceeds to INSERT — straight into the base table's
-- UNIQUE(content_hash). Dropping the tombstone first restores v10's own
-- historical behaviour exactly: re-importing a removed book gives a new
-- row with fresh history (the old row's sessions and bookmarks cascade
-- away with it). A partial unique index would instead let two rows share a
-- hash and strand the tombstone for good. The tombstone is not live, so
-- `publications_soft_delete` does not fire and the delete is real.
CREATE TRIGGER publications_view_insert
INSTEAD OF INSERT ON publications
BEGIN
  DELETE FROM publications_all
   WHERE NEW.content_hash IS NOT NULL
     AND content_hash = NEW.content_hash
     AND removed_at IS NOT NULL;
  INSERT INTO publications_all
    (id, title, authors, language, format, file_path, added_at, progression,
     content_hash, cover_path, finished_at, last_opened_at, locator,
     position_count, text_encoding, position_spine_idx, position_char_offset,
     reconciled_at)
  VALUES
    (NEW.id, NEW.title, NEW.authors, NEW.language, NEW.format, NEW.file_path,
     NEW.added_at, NEW.progression, NEW.content_hash, NEW.cover_path,
     NEW.finished_at, NEW.last_opened_at, NEW.locator, NEW.position_count,
     NEW.text_encoding, NEW.position_spine_idx, NEW.position_char_offset,
     NEW.reconciled_at);
END;
";

// 0012: edition identity, so the finished-books stat counts editions rather
// than rows. `content_hash` cannot see that a book you finished, removed,
// and re-imported as a differently-encoded copy is one book you finished —
// the two files differ byte for byte — so `finished_at` counted it twice.
//
// `edition_key` is the normalized `dc:identifier` (UUID / checksum-valid
// ISBN / DOI, nothing else — see `features/library/edition.rs`) and
// `title_key` is the normalized title; the stat merges only on BOTH, so one
// hardcoded `urn:uuid:` stamped across a publisher's whole catalogue still
// cannot collapse a shelf. NULL in either means "no identity", and a book
// with no identity counts as itself — the safe direction.
//
// `edition_scanned_at` is the backfill gate, not a timestamp anyone reads:
// filling `edition_key` needs the book's OPF, which a migration cannot
// re-open for every row, so a bounded background pass does it (chained
// after the V8 rebaseline) and stamps every book it looked at, success or
// failure, so an unreadable file is not retried on every open forever.
// Tombstones have no file left and are never scanned; they keep
// `edition_key` NULL and count as themselves.
//
// `title_key` IS backfilled here, in the migration: it is a pure function
// of the stored title with no file access at all.
//
// The partial index coexists with V11's triggers — a column add fires no
// row trigger, and neither trigger references these columns. The columns
// land on `publications_all` (V11 renamed the table out from under the
// `publications` name, which is now a view); the view is deliberately
// v10-shaped and needs no change, which is the whole point of listing its
// columns explicitly.
const V12_SQL: &str = "
ALTER TABLE publications_all ADD COLUMN edition_key        TEXT;
ALTER TABLE publications_all ADD COLUMN title_key          TEXT;
ALTER TABLE publications_all ADD COLUMN edition_scanned_at INTEGER;

CREATE INDEX idx_publications_edition ON publications_all(edition_key)
  WHERE edition_key IS NOT NULL;
";

pub(crate) fn migrate(conn: &mut Connection, data_dir: &Path) -> Result<(), CoreError> {
    migrate_upto(conn, data_dir, SCHEMA_VERSION)
}

/// Test-only: runs the real migration chain but stops at `target`, so a
/// test can seed a database at an intermediate shipped version without
/// duplicating shipped SQL.
#[cfg(test)]
pub(super) fn migrate_to(
    conn: &mut Connection,
    data_dir: &Path,
    target: i64,
) -> Result<(), CoreError> {
    migrate_upto(conn, data_dir, target)
}

fn migrate_upto(conn: &mut Connection, data_dir: &Path, target: i64) -> Result<(), CoreError> {
    loop {
        let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version >= target {
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
            2 => tx.execute_batch(V3_SQL)?,
            3 => tx.execute_batch(V4_SQL)?,
            4 => tx.execute_batch(V5_SQL)?,
            5 => tx.execute_batch(V6_SQL)?,
            6 => tx.execute_batch(V7_SQL)?,
            7 => tx.execute_batch(V8_SQL)?,
            8 => tx.execute_batch(V9_SQL)?,
            9 => tx.execute_batch(V10_SQL)?,
            10 => tx.execute_batch(V11_SQL)?,
            11 => {
                tx.execute_batch(V12_SQL)?;
                backfill_title_keys(&tx)?;
            }
            // The loop guard makes other values impossible.
            _ => return Ok(()),
        }
        tx.pragma_update(None, "user_version", version + 1)?;
        tx.commit()?;
    }
}

/// Fills `title_key` for every existing row from the title already stored,
/// inside the V12 transaction: a pure normalization, no file access, so
/// unlike `edition_key` it needs no background pass.
///
/// V11's `publications_freeze_tombstone` trigger silently drops the write
/// for a tombstone. That is harmless rather than a gap: a tombstone's file
/// is gone, so its `edition_key` stays NULL forever, and the stat merges
/// only when BOTH keys are present — a tombstone counts as itself either
/// way. The loop still covers every row so nothing depends on that trigger
/// staying as it is.
fn backfill_title_keys(tx: &Transaction) -> Result<(), CoreError> {
    let rows: Vec<(String, String)> = {
        let mut stmt = tx.prepare("SELECT id, title FROM publications_all")?;
        let mapped = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        mapped.collect::<Result<_, _>>()?
    };
    let mut update = tx.prepare("UPDATE publications_all SET title_key = ?1 WHERE id = ?2")?;
    for (id, title) in rows {
        update.execute(rusqlite::params![title_key(&title), id])?;
    }
    Ok(())
}

/// Adopts every v1 row into core-owned storage: copy the external file into
/// `books/<id>.<format>`, compute its BLAKE3 hash, rewrite `file_path`
/// relative. Rows whose source file is missing or unreadable — or whose
/// content duplicates an already-adopted row — are dropped. After this,
/// every surviving row is core-owned: `remove()` is always safe and dedupe
/// always has a hash.
///
/// Runs inside the V2 step, where `publications` is still the base table —
/// V11 is what renames it to `publications_all` — so these statements name
/// it as it was and must stay that way.
fn adopt_legacy_rows(tx: &Transaction, data_dir: &Path) -> Result<(), CoreError> {
    let rows: Vec<(String, String, String)> = {
        let mut stmt = tx.prepare("SELECT id, file_path, format FROM publications")?;
        let mapped = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
        mapped.collect::<Result<_, _>>()?
    };

    let mut seen = HashSet::new();
    for (id, file_path, format) in rows {
        let rel = format!("books/{id}.{format}");
        let dest = data_dir.join(&rel);
        // Unbounded on purpose: the import ceiling guards new external
        // streams, and dropping a legacy row because its book is large
        // would be silent data loss at the one moment nobody could refuse.
        match copy_and_hash_unbounded(Path::new(&file_path), &dest) {
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
