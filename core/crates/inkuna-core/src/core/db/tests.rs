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
            "INSERT INTO publications VALUES ('live-id', '生きてる本', '著者', 'ja', 'epub', ?1, 100, 0.5)",
            [alive.to_str().unwrap()],
        )
        .unwrap();
        conn.execute(
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
        "SELECT COUNT(position_spine_idx) FROM publications",
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
