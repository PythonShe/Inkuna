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
        Err(_) => panic!("reader pool starved: pooled reads blocked forever after panics in `work`"),
    }
}
