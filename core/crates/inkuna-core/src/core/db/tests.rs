use rusqlite::Connection;

use crate::test_support::write_epub;
use crate::{ImportOutcome, Library, Shelf, Sort};

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

