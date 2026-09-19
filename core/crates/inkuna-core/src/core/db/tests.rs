use std::panic::AssertUnwindSafe;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use rusqlite::Connection;

use super::READER_POOL_SIZE;
use crate::test_support::write_epub;
use crate::{CoreError, ImportOutcome, Library, Shelf, Sort};

#[test]
fn migration_adds_nullable_text_encoding_to_an_existing_database() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("library");
    std::fs::create_dir_all(&data_dir).unwrap();
    {
        let conn = Connection::open(data_dir.join("inkuna.db")).unwrap();
        conn.execute_batch(
            "CREATE TABLE publications (
                id          TEXT PRIMARY KEY,
                title       TEXT NOT NULL,
                authors     TEXT NOT NULL DEFAULT '',
                language    TEXT,
                format      TEXT NOT NULL,
                file_path   TEXT NOT NULL,
                added_at    INTEGER NOT NULL,
                progression REAL NOT NULL DEFAULT 0
            );",
        )
        .unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
    }

    let library = Library::open(&data_dir).unwrap();
    let text_encoding_column: Option<(i64, Option<String>)> = library
        .readers
        .with(|conn| {
            let mut statement = conn.prepare("PRAGMA table_info(publications)")?;
            let columns = statement.query_map([], |row| {
                Ok((
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            })?;
            Ok(columns
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .find_map(|(name, not_null, default)| {
                    (name == "text_encoding").then_some((not_null, default))
                }))
        })
        .unwrap();
    assert_eq!(text_encoding_column, Some((0, None)));
}

#[test]
fn migration_adopts_live_rows_and_drops_dead_ones() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("library");
    std::fs::create_dir_all(&data_dir).unwrap();

    let alive = dir.path().join("alive.epub");
    write_epub(&alive, "生きてる本", "著者", "ja");

    // Hand-build a v1 database: external absolute paths, schema v1.
    {
        let conn = Connection::open(data_dir.join("inkuna.db")).unwrap();
        conn.execute_batch(
            "CREATE TABLE publications (
                id          TEXT PRIMARY KEY,
                title       TEXT NOT NULL,
                authors     TEXT NOT NULL DEFAULT '',
                language    TEXT,
                format      TEXT NOT NULL,
                file_path   TEXT NOT NULL,
                added_at    INTEGER NOT NULL,
                progression REAL NOT NULL DEFAULT 0
            );",
        )
        .unwrap();
        conn.execute(
            // v10-view-sql: a v1 fixture — `publications` is still the table here.
            "INSERT INTO publications VALUES ('live-id', '生きてる本', '著者', 'ja', 'epub', ?1, 100, 0.5)",
            [alive.to_str().unwrap()],
        )
        .unwrap();
        conn.execute(
            // v10-view-sql: same v1 fixture, the row the adoption pass drops.
            "INSERT INTO publications VALUES ('dead-id', 'Gone', '', NULL, 'epub', '/no/such/file.epub', 200, 0.0)",
            [],
        )
        .unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
    }

    let library = Library::open(&data_dir).unwrap();
    let listed = library.list(Shelf::All, Sort::RecentlyAdded).unwrap();
    assert_eq!(listed.len(), 1);
    let adopted = &listed[0];
    assert_eq!(adopted.id, "live-id");
    assert_eq!(adopted.title, "生きてる本");
    assert_eq!(adopted.progression, 0.5);
    // Adopted: copied in, relativized, hashed (dedupe now works on it).
    assert_eq!(adopted.file_path, "books/live-id.epub");
    assert!(data_dir.join("books/live-id.epub").is_file());
    match library.import(alive.to_str().unwrap()).unwrap() {
        ImportOutcome::Duplicate(p) => assert_eq!(p.id, "live-id"),
        other => panic!("expected duplicate of adopted row, got {other:?}"),
    }
}

#[test]
fn v8_migrates_from_v7() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("library");
    std::fs::create_dir_all(&data_dir).unwrap();
    let db_path = data_dir.join("inkuna.db");

    // A real v7 database with a publication and a bookmark, via the
    // shipped migration chain stopped at 7.
    {
        let mut conn = super::open_connection(&db_path).unwrap();
        super::migrate::migrate_to(&mut conn, &data_dir, 7).unwrap();
        conn.execute(
            // v10-view-sql: a v7 fixture, written before the view existed.
            "INSERT INTO publications
                (id, title, authors, format, file_path, added_at, progression, locator)
             VALUES ('p1', '月光書房', '紫式部', 'epub', 'books/p1.epub', 100, 0.5,
                     '{\"href\":\"OEBPS/ch01.xhtml\"}')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO bookmarks (id, publication_id, locator, progression, created_at)
             VALUES ('b1', 'p1', '{\"href\":\"OEBPS/ch01.xhtml\"}', 0.25, 200)",
            [],
        )
        .unwrap();
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 7);
    }

    let mut conn = super::open_connection(&db_path).unwrap();
    super::migrate::migrate_to(&mut conn, &data_dir, 8).unwrap();
    let version: i64 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 8);

    // New columns exist and are NULL; old data is intact.
    let (title, locator, spine_idx, char_offset, reconciled_at): (
        String,
        Option<String>,
        Option<i64>,
        Option<i64>,
        Option<i64>,
    ) = conn
        .query_row(
            // v10-view-sql: read at v8, where `publications` is the table.
            "SELECT title, locator, position_spine_idx, position_char_offset, reconciled_at
             FROM publications WHERE id = 'p1'",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(title, "月光書房");
    assert_eq!(locator.as_deref(), Some("{\"href\":\"OEBPS/ch01.xhtml\"}"));
    assert_eq!(spine_idx, None);
    assert_eq!(char_offset, None);
    assert_eq!(reconciled_at, None);

    let (bm_locator, bm_spine_idx, bm_char_offset): (String, Option<i64>, Option<i64>) = conn
        .query_row(
            "SELECT locator, position_spine_idx, position_char_offset
             FROM bookmarks WHERE id = 'b1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(bm_locator, "{\"href\":\"OEBPS/ch01.xhtml\"}");
    assert_eq!(bm_spine_idx, None);
    assert_eq!(bm_char_offset, None);
}

#[test]
fn fresh_install_reaches_latest_schema() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("library");
    std::fs::create_dir_all(&data_dir).unwrap();

    let mut conn = super::open_connection(&data_dir.join("inkuna.db")).unwrap();
    super::migrate(&mut conn, &data_dir).unwrap();
    let version: i64 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, super::migrate::SCHEMA_VERSION);
    // The coordinate columns are queryable on a fresh install.
    conn.query_row(
        "SELECT COUNT(position_spine_idx) FROM publications_all",
        [],
        |row| row.get::<_, i64>(0),
    )
    .unwrap();
}

/// A v8 database — settings row included — gains the haptics column with
/// haptics on, the default every install so far has lived with.
#[test]
fn v9_migrates_from_v8() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("library");
    std::fs::create_dir_all(&data_dir).unwrap();
    let db_path = data_dir.join("inkuna.db");

    {
        let mut conn = super::open_connection(&db_path).unwrap();
        super::migrate::migrate_to(&mut conn, &data_dir, 8).unwrap();
        conn.execute("UPDATE settings SET reading_theme = 'moon'", [])
            .unwrap();
    }

    let mut conn = super::open_connection(&db_path).unwrap();
    super::migrate(&mut conn, &data_dir).unwrap();
    let (theme, haptics): (String, bool) = conn
        .query_row(
            "SELECT reading_theme, haptics FROM settings WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(theme, "moon");
    assert!(haptics);
}

/// A v9 database — settings row included — gains the library_grid column
/// with the grid off, the list every install so far has rendered.
#[test]
fn v10_migrates_from_v9() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("library");
    std::fs::create_dir_all(&data_dir).unwrap();
    let db_path = data_dir.join("inkuna.db");

    {
        let mut conn = super::open_connection(&db_path).unwrap();
        super::migrate::migrate_to(&mut conn, &data_dir, 9).unwrap();
        conn.execute("UPDATE settings SET reading_theme = 'moon'", [])
            .unwrap();
    }

    let mut conn = super::open_connection(&db_path).unwrap();
    super::migrate(&mut conn, &data_dir).unwrap();
    let (theme, library_grid): (String, bool) = conn
        .query_row(
            "SELECT reading_theme, library_grid FROM settings WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(theme, "moon");
    assert!(!library_grid);
}

/// The one migration where a wrong default is catastrophic rather than
/// cosmetic: `removed_at` is what every library read now filters on, so a
/// non-NULL default would tombstone an existing user's entire library —
/// every book gone from every shelf, from search, from the reader — behind
/// a migration they cannot undo. A populated v10 database must come
/// through it with its books, its history, and its shelves untouched.
#[test]
fn v11_migrates_from_v10() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("library");
    std::fs::create_dir_all(&data_dir).unwrap();
    let db_path = data_dir.join("inkuna.db");

    {
        let mut conn = super::open_connection(&db_path).unwrap();
        super::migrate::migrate_to(&mut conn, &data_dir, 10).unwrap();
        conn.execute(
            // v10-view-sql: a v10 fixture — the row this migration renames.
            "INSERT INTO publications
                (id, title, authors, format, file_path, content_hash, added_at,
                 progression, position_spine_idx, position_char_offset, finished_at)
             VALUES ('p1', '月光書房', '紫式部', 'epub', 'books/p1.epub', 'hash-1', 100,
                     0.5, 1, 7, 900)",
            [],
        )
        .unwrap();
        conn.execute(
            // `bookmarks.locator` is still NOT NULL at v10; a rebaselined
            // bookmark carries both it and its coordinates.
            "INSERT INTO bookmarks
                (id, publication_id, locator, progression, created_at,
                 position_spine_idx, position_char_offset)
             VALUES ('b1', 'p1', '{\"href\":\"OEBPS/ch01.xhtml\"}', 0.25, 200, 0, 3)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sessions
                (id, publication_id, started_at, ended_at, updated_at,
                 start_progression, end_progression)
             VALUES ('s1', 'p1', 300, 900, 900, 0.1, 0.5)",
            [],
        )
        .unwrap();
    }

    let mut conn = super::open_connection(&db_path).unwrap();
    // Stopped at 11: this test is about the v10→v11 step alone, and the
    // `Library::open` at the end still runs the chain out to the latest.
    super::migrate::migrate_to(&mut conn, &data_dir, 11).unwrap();
    let version: i64 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 11);

    // Both new columns default to NULL. `removed_at` NULL is what "live"
    // means, and `corpus_digest` NULL is unknown provenance — a book that
    // was never removed has no coordinates frozen across a removal.
    let (removed_at, corpus_digest): (Option<i64>, Option<String>) = conn
        .query_row(
            "SELECT removed_at, corpus_digest FROM publications_all WHERE id = 'p1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(removed_at, None, "an existing book is LIVE, not removed");
    assert_eq!(corpus_digest, None);

    // And nothing the migration ran over moved.
    let (title, spine_idx, char_offset, finished_at): (
        String,
        Option<i64>,
        Option<i64>,
        Option<i64>,
    ) = conn
        .query_row(
            "SELECT title, position_spine_idx, position_char_offset, finished_at
             FROM publications_all WHERE id = 'p1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(title, "月光書房");
    assert_eq!(spine_idx, Some(1), "the reading position is untouched");
    assert_eq!(char_offset, Some(7));
    assert_eq!(finished_at, Some(900));

    // The history hanging off it survives the migration whole.
    let (bm_spine_idx, bm_progression): (Option<i64>, f64) = conn
        .query_row(
            "SELECT position_spine_idx, progression FROM bookmarks WHERE id = 'b1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(bm_spine_idx, Some(0));
    assert_eq!(bm_progression, 0.25);
    let sessions: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sessions WHERE publication_id = 'p1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(sessions, 1);

    // And the book is still on the shelf — the assertion that would have
    // caught a bad default from the user's side of the screen.
    drop(conn);
    let library = Library::open(&data_dir).unwrap();
    let shelf = library.list(Shelf::All, Sort::RecentlyAdded).unwrap();
    assert_eq!(shelf.len(), 1, "migrating must not empty the library");
    assert_eq!(shelf[0].id, "p1");
    assert_eq!(shelf[0].progression, 0.5);
    assert_eq!(library.bookmarks("p1").unwrap().len(), 1);
}

/// V12 adds the edition-identity columns and backfills `title_key` for
/// every row it can — a pure normalization of the stored title. The two
/// columns that need a file (`edition_key`) or a pass (`edition_scanned_at`)
/// stay NULL, which is what leaves the background backfill work to do.
#[test]
fn v12_migrates_from_v11() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("library");
    std::fs::create_dir_all(&data_dir).unwrap();
    let db_path = data_dir.join("inkuna.db");

    {
        let mut conn = super::open_connection(&db_path).unwrap();
        super::migrate::migrate_to(&mut conn, &data_dir, 11).unwrap();
        conn.execute(
            // The tombstone below goes to `publications_all` on purpose;
            // v10-view-sql: this one is LIVE, inserted through the view.
            "INSERT INTO publications
                (id, title, authors, format, file_path, content_hash, added_at,
                 progression, finished_at)
             VALUES ('p1', '　月光　書房　', '紫式部', 'epub', 'books/p1.epub', 'hash-1',
                     100, 0.5, 900)",
            [],
        )
        .unwrap();
        // A tombstone, whose title V11's freeze trigger protects from any
        // write that leaves it a tombstone — the backfill included.
        conn.execute(
            "INSERT INTO publications_all
                (id, title, authors, format, file_path, content_hash, added_at,
                 progression, removed_at)
             VALUES ('p2', 'Gone', '', 'epub', '', 'hash-2', 100, 0.5, 500)",
            [],
        )
        .unwrap();
    }

    let mut conn = super::open_connection(&db_path).unwrap();
    super::migrate(&mut conn, &data_dir).unwrap();
    let version: i64 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 12);
    assert_eq!(version, super::migrate::SCHEMA_VERSION);

    let (edition_key, title_key, scanned_at, title, finished_at): (
        Option<String>,
        Option<String>,
        Option<i64>,
        String,
        Option<i64>,
    ) = conn
        .query_row(
            "SELECT edition_key, title_key, edition_scanned_at, title, finished_at
             FROM publications_all WHERE id = 'p1'",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .unwrap();
    // Backfilled in the migration: NFKC folds the ideographic spaces to
    // ASCII ones, which the key then strips.
    assert_eq!(title_key.as_deref(), Some("月光書房"));
    // Not backfilled: filling it needs the book's OPF.
    assert_eq!(edition_key, None);
    assert_eq!(scanned_at, None, "the background pass still has work to do");
    // And nothing the migration ran over moved.
    assert_eq!(title, "　月光　書房　");
    assert_eq!(finished_at, Some(900));

    // The tombstone comes through frozen, which is harmless: its
    // `edition_key` can never be filled either, so it counts as itself.
    let (tomb_title_key, removed_at): (Option<String>, Option<i64>) = conn
        .query_row(
            "SELECT title_key, removed_at FROM publications_all WHERE id = 'p2'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(tomb_title_key, None);
    assert_eq!(removed_at, Some(500), "the tombstone is still a tombstone");

    // The partial index is live alongside V11's triggers.
    let indexes: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'index' AND name = 'idx_publications_edition'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(indexes, 1);
    let triggers: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        triggers, 5,
        "V11's two base-table triggers and three INSTEAD OF ones all survive the column adds"
    );

    // The compatibility view stays v10-shaped: V12's columns land on the
    // base table and must not widen what an old binary sees.
    let view_columns: Vec<String> = {
        let mut stmt = conn.prepare("PRAGMA table_info(publications)").unwrap();
        let rows = stmt.query_map([], |row| row.get::<_, String>(1)).unwrap();
        rows.collect::<Result<_, _>>().unwrap()
    };
    for added in ["edition_key", "title_key", "edition_scanned_at"] {
        assert!(
            !view_columns.contains(&added.to_string()),
            "{added} must not reach the v10 view"
        );
    }

    // And the library still opens on the live book.
    drop(conn);
    let library = Library::open(&data_dir).unwrap();
    let shelf = library.list(Shelf::All, Sort::RecentlyAdded).unwrap();
    assert_eq!(shelf.len(), 1);
    assert_eq!(shelf[0].id, "p1");
}

/// The forward gate. Migrations are append-only and forward-only, so a
/// `user_version` past this build's is a database written by a newer
/// Inkuna: there is nothing to run, and reading it anyway is exactly the
/// failure V11 had to spend a compatibility view containing. `Library::open`
/// must refuse it rather than open a library it would misread.
#[test]
fn migrate_refuses_a_future_schema() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("library");
    std::fs::create_dir_all(&data_dir).unwrap();
    let db_path = data_dir.join("inkuna.db");

    {
        let mut conn = super::open_connection(&db_path).unwrap();
        super::migrate(&mut conn, &data_dir).unwrap();
        // A schema from a build that does not exist yet.
        conn.pragma_update(None, "user_version", 99).unwrap();
    }

    let mut conn = super::open_connection(&db_path).unwrap();
    match super::migrate(&mut conn, &data_dir) {
        Err(CoreError::SchemaTooNew { found, supported }) => {
            assert_eq!(found, 99);
            assert_eq!(supported, super::migrate::SCHEMA_VERSION);
            assert_eq!(supported, 12);
        }
        other => panic!("a future schema must be refused, got {other:?}"),
    }
    // Refusing changes nothing: the newer build's schema is left exactly as
    // it was found, so the newer build still opens its own library.
    let version: i64 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 99);

    // And the refusal is what a shell sees, not a panic or an empty shelf.
    drop(conn);
    match Library::open(&data_dir) {
        Err(CoreError::SchemaTooNew { found, supported }) => {
            assert_eq!(found, 99);
            assert_eq!(supported, 12);
        }
        Err(other) => panic!("`Library::open` must propagate the refusal, got {other:?}"),
        Ok(_) => panic!("`Library::open` opened a database from a newer build"),
    }
}

/// Since V11 the real table is `publications_all` and `publications` is a
/// v10-shaped view over the live rows — so SQL that names `publications`
/// compiles, passes, and silently skips every tombstone. Nothing in the
/// language stops that, so this walks the crate's own source and stops it
/// here.
///
/// The opt-out is deliberate and per-statement: a test that means to speak
/// v10 through the view says so with a `v10-view-sql` marker on the line or
/// the line above it. Skipping test files wholesale would cost the guard
/// its teeth exactly where the footgun is easiest to fire.
#[test]
fn no_sql_bypasses_the_tombstone_view() {
    /// The word that excuses a match, for SQL that means the view.
    const MARKER: &str = "v10-view-sql";
    /// How far above a match the marker may sit.
    const MARKER_LOOKBACK: usize = 3;
    /// A bare `publications` after one of these is a statement against the
    /// view, whatever the author meant.
    const KEYWORDS: [&str; 4] = ["from", "into", "update", "join"];

    fn rs_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                rs_files(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }

    /// Splits on everything SQL and Rust use as punctuation, so `publications`
    /// is a token of its own while `publications_all` stays one word.
    fn words(line: &str) -> Vec<String> {
        line.split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|w| !w.is_empty())
            .map(|w| w.to_ascii_lowercase())
            .collect()
    }

    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rs_files(&src, &mut files);
    files.sort();

    let mut offenders = Vec::new();
    for file in files {
        // The migrations legitimately name both: every step before V11 runs
        // when `publications` IS the table, and V11 itself creates the view.
        if file.ends_with("core/db/migrate.rs") {
            continue;
        }
        let text = std::fs::read_to_string(&file).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        for (idx, line) in lines.iter().enumerate() {
            // Prose may name the view freely; only SQL is the hazard.
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("--") {
                continue;
            }
            let tokens = words(line);
            let hit = tokens
                .windows(2)
                .any(|pair| KEYWORDS.contains(&pair[0].as_str()) && pair[1] == "publications");
            if !hit {
                continue;
            }
            // The match can land on a continuation line of a multi-line SQL
            // literal, so the marker is allowed to sit a little above it —
            // where the statement starts — not on that one line alone.
            let excused = line.contains(MARKER)
                || lines[idx.saturating_sub(MARKER_LOOKBACK)..idx]
                    .iter()
                    .any(|above| above.contains(MARKER));
            if !excused {
                let relative = file.strip_prefix(&src).unwrap_or(&file);
                offenders.push(format!(
                    "  {}:{}: {}",
                    relative.display(),
                    idx + 1,
                    line.trim()
                ));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "SQL naming the bare table `publications`:\n{}\n\n\
         Since V11 `publications` is a v10 compatibility VIEW over the live \
         rows only — it hides every tombstone (`removed_at IS NOT NULL`), so \
         this SQL silently skips removed books and a removed book can never \
         be found, counted, or restored. Name `publications_all` instead. If \
         the statement really does mean the v10 view, put a `{MARKER}` \
         comment on that line or the line above it.",
        offenders.join("\n")
    );
}

/// V11 renames `publications` out from under five child tables and rests
/// the whole rename on SQLite rewriting their `REFERENCES` clauses. If it
/// did not, each clause would keep naming `publications` — a view since
/// V11 — and every child insert would die with `foreign key mismatch`,
/// unrepairably, because `user_version` is already past the step. Nothing
/// else in the crate asserts the rewrite happened, so this does.
#[test]
fn child_tables_reference_the_renamed_base_table() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("library");
    std::fs::create_dir_all(&data_dir).unwrap();
    let mut conn = super::open_connection(&data_dir.join("inkuna.db")).unwrap();
    super::migrate(&mut conn, &data_dir).unwrap();

    for child in [
        "sessions",
        "bookmarks",
        "resources",
        "chapters",
        "resource_positions",
    ] {
        let parent: String = conn
            .query_row(
                &format!("SELECT \"table\" FROM pragma_foreign_key_list('{child}')"),
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            parent, "publications_all",
            "{child} must reference the base table, not V11's view"
        );
    }

    // And the rewrite is load-bearing at runtime, not just in the schema
    // text: a child insert against a live book resolves its parent.
    conn.execute(
        "INSERT INTO publications_all
            (id, title, authors, format, file_path, added_at, progression)
         VALUES ('p1', 'Book', '', 'epub', 'books/p1.epub', 100, 0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO sessions
            (id, publication_id, started_at, updated_at,
             start_progression, end_progression)
         VALUES ('s1', 'p1', 1, 1, 0, 0.5)",
        [],
    )
    .unwrap();
    // A dangling parent is still refused, so the constraint is enforced
    // rather than merely present.
    assert!(
        conn.execute(
            "INSERT INTO sessions
                (id, publication_id, started_at, updated_at,
                 start_progression, end_progression)
             VALUES ('s2', 'ghost', 1, 1, 0, 0.5)",
            [],
        )
        .is_err(),
        "the foreign key must still bite"
    );
}

/// The rewrite above is a side effect of the rename that SQLite only
/// guarantees while `PRAGMA foreign_keys` is on, and with it off the
/// rename succeeds anyway — leaving a database no later open can repair,
/// because the step has committed and `user_version` has moved on. (The
/// same pragma is what lets `publications_soft_delete` clear a removed
/// book's corpus through `resource_text`'s cascade, so it is doubly
/// required.) `open_connection` enables it, but nothing in the schema
/// depended on that until V11, and `deadpool-sqlite` — designated for the
/// concurrent-DB work — would build its own connections, so the step
/// refuses rather than rest on a setting made in another module.
#[test]
fn v11_refuses_the_rename_without_foreign_keys() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("library");
    std::fs::create_dir_all(&data_dir).unwrap();
    let db_path = data_dir.join("inkuna.db");

    {
        let mut conn = super::open_connection(&db_path).unwrap();
        super::migrate::migrate_to(&mut conn, &data_dir, 10).unwrap();
    }

    // A connection with the pragma off — what a stock SQLite build gives
    // by default, and what any future connection setup that forgets it
    // would give here (`deadpool-sqlite` is already designated for the
    // concurrent-DB work). rusqlite's bundled build defaults it on, so the
    // fixture turns it off explicitly rather than relying on that.
    let mut conn = Connection::open(&db_path).unwrap();
    conn.pragma_update(None, "foreign_keys", "OFF").unwrap();
    let enabled: bool = conn
        .pragma_query_value(None, "foreign_keys", |row| row.get(0))
        .unwrap();
    assert!(!enabled, "the fixture must have foreign keys off");

    match super::migrate(&mut conn, &data_dir) {
        Err(CoreError::MigrationPrecondition(detail)) => {
            assert!(
                detail.contains("foreign_keys"),
                "the refusal must name the precondition, got {detail}"
            );
        }
        other => panic!("v11 must refuse a connection without foreign keys, got {other:?}"),
    }

    // Refusing leaves the database exactly as it was found: still v10,
    // `publications` still the real table, so reopening with a correct
    // connection migrates it properly.
    let version: i64 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 10);
    let kind: String = conn
        .query_row(
            "SELECT type FROM sqlite_master WHERE name = 'publications'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(kind, "table", "the rename must not have happened");

    drop(conn);
    let mut conn = super::open_connection(&db_path).unwrap();
    super::migrate(&mut conn, &data_dir).unwrap();
    let version: i64 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, super::migrate::SCHEMA_VERSION);
}

/// The belt to that brace. The rewrite is a side effect nothing in the
/// statement asks for — gated on pragmas and on the SQLite version, which
/// before 3.25 did not do it at all — so V11 also checks the outcome
/// before it is allowed to commit, and refuses rather than leave a
/// database whose child inserts all fail with `foreign key mismatch`
/// forever after. A v10 schema, where the children genuinely still
/// reference `publications`, is exactly the shape a rename that did not
/// rewrite would leave behind.
#[test]
fn the_v11_guard_rejects_child_references_left_behind() {
    fn guard_at(stage: i64) -> Result<(), CoreError> {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("library");
        std::fs::create_dir_all(&data_dir).unwrap();
        let mut conn = super::open_connection(&data_dir.join("inkuna.db")).unwrap();
        super::migrate::migrate_to(&mut conn, &data_dir, stage).unwrap();
        let tx = conn.transaction().unwrap();
        super::migrate::require_renamed_child_references(&tx)
    }

    // v11 onwards: the rename happened, so the guard passes.
    guard_at(11).expect("a renamed schema must satisfy the guard");

    // v10: the children reference `publications`, which is what a rename
    // that did not rewrite would leave under V11's view.
    match guard_at(10) {
        Err(CoreError::MigrationPrecondition(detail)) => {
            assert!(
                detail.contains("sessions") && detail.contains("publications"),
                "the refusal must name what was left behind, got {detail}"
            );
        }
        other => panic!("the guard must reject an unrewritten schema, got {other:?}"),
    }
}

/// A panic inside pooled work must not consume the connection. UniFFI
/// catches panics at the boundary and keeps the app alive, so leaking one
/// connection per panic would silently starve the pool and then block every
/// later read forever.
#[test]
fn reader_pool_outlives_panicking_work() {
    let dir = tempfile::tempdir().unwrap();
    let library = Arc::new(Library::open(dir.path().join("library")).unwrap());

    // The whole exercise runs off-thread behind a timeout: a pool that
    // loses connections to panics blocks forever rather than erroring, so
    // the timeout is what turns a regression into a failure instead of a
    // hung suite.
    let (tx, rx) = mpsc::channel();
    let worker = Arc::clone(&library);
    std::thread::spawn(move || {
        // More panics than the pool has connections.
        for _ in 0..READER_POOL_SIZE + 2 {
            let panicked = std::panic::catch_unwind(AssertUnwindSafe(|| {
                worker
                    .readers
                    .with(|_conn| -> Result<(), CoreError> { panic!("read blew up") })
            }));
            if panicked.is_ok() {
                let _ = tx.send(Err("the panic never reached the caller".to_string()));
                return;
            }
        }
        // Every connection must still be handing out reads.
        let value = worker
            .readers
            .with(|conn| {
                conn.query_row("SELECT 1", [], |row| row.get::<_, i64>(0))
                    .map_err(Into::into)
            })
            .map_err(|e: CoreError| e.to_string());
        let _ = tx.send(value);
    });

    match rx.recv_timeout(Duration::from_secs(10)) {
        Ok(Ok(1)) => {}
        Ok(other) => panic!("unexpected read result: {other:?}"),
        Err(_) => {
            panic!("reader pool starved: pooled reads blocked forever after panics in `work`")
        }
    }
}
