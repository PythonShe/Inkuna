use std::sync::atomic::AtomicBool;

use crate::test_support::write_epub_parts;
use crate::{ImportOutcome, Library};

const UUID: &str = "urn:uuid:5c9b2f1e-8a3d-4f7b-9e2c-1d0a6b4f8e37";
const KEY: &str = "uuid:5c9b2f1e-8a3d-4f7b-9e2c-1d0a6b4f8e37";

/// Imports a book carrying `identifier` and rewinds it to the pre-V12
/// state the migration leaves behind: both edition columns NULL, so the
/// pass has work to do. Returns `(dir, library, id)`.
fn unscanned_book(identifier: &str) -> (tempfile::TempDir, Library, String) {
    let dir = tempfile::tempdir().unwrap();
    let epub = dir.path().join("book.epub");
    write_identified(&epub, identifier, "本文");

    let library = Library::open(dir.path().join("library")).unwrap();
    // Join the open-spawned pass before mutating rows, so nothing races
    // the manual runs below.
    library.search.wait_for_reconcile();
    let id = match library.import(epub.to_str().unwrap()).unwrap() {
        ImportOutcome::Imported(p) => p.id,
        other => panic!("unexpected {other:?}"),
    };
    unscan(&library, &id);
    (dir, library, id)
}

/// A minimal EPUB carrying a hand-written `dc:identifier`; `filler` varies
/// only the chapter bytes, so two calls sharing one identifier still
/// produce two differently-hashed files.
fn write_identified(path: &std::path::Path, identifier: &str, filler: &str) {
    let opf = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="pub-id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>月光書房</dc:title>
    <dc:identifier id="pub-id">{identifier}</dc:identifier>
    <dc:language>ja</dc:language>
  </metadata>
  <manifest><item id="c1" href="ch01.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="c1"/></spine>
</package>"#
    );
    let chapter = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><body><p>{filler}</p></body></html>"#
    );
    write_epub_parts(path, &opf, &[("ch01.xhtml", &chapter)]);
}

/// Rewinds one row to the pre-V12 state: no key, never scanned.
fn unscan(library: &Library, id: &str) {
    let conn = library.writer.lock().unwrap();
    conn.execute(
        "UPDATE publications_all
            SET edition_key = NULL, edition_scanned_at = NULL
          WHERE id = ?1",
        [id],
    )
    .unwrap();
}

fn run(library: &Library) {
    super::run(
        &library.data_dir,
        &library.data_dir.join("inkuna.db"),
        &AtomicBool::new(false),
    );
}

fn edition_row(library: &Library, id: &str) -> (Option<String>, Option<i64>) {
    library
        .readers
        .with(|conn| {
            conn.query_row(
                "SELECT edition_key, edition_scanned_at FROM publications_all WHERE id = ?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(Into::into)
        })
        .unwrap()
}

fn pending_count(library: &Library) -> i64 {
    library
        .readers
        .with(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM publications_all
                 WHERE edition_scanned_at IS NULL AND removed_at IS NULL",
                [],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .unwrap()
}

#[test]
fn a_pre_v12_book_is_keyed_by_the_pass() {
    let (_dir, library, id) = unscanned_book(UUID);
    assert_eq!(pending_count(&library), 1);

    run(&library);

    let (key, scanned_at) = edition_row(&library, &id);
    assert_eq!(key.as_deref(), Some(KEY));
    assert!(scanned_at.is_some());
    assert_eq!(
        pending_count(&library),
        0,
        "the pass is done with this book"
    );
}

#[test]
fn a_junk_identifier_is_stamped_with_no_identity() {
    let (_dir, library, id) = unscanned_book("calibre_id");

    run(&library);

    let (key, scanned_at) = edition_row(&library, &id);
    assert_eq!(key, None, "a junk identifier is no identity");
    assert!(
        scanned_at.is_some(),
        "but the book was looked at, so it is not pending"
    );
    assert_eq!(pending_count(&library), 0);
}

/// The stamp is what makes an unreadable file a one-time cost. Without it
/// the same broken archive would be re-opened on every launch, forever.
#[test]
fn an_unreadable_book_is_stamped_and_not_retried() {
    let (_dir, library, id) = unscanned_book(UUID);
    let stored = library.data_dir.join(format!("books/{id}.epub"));
    std::fs::write(&stored, b"not an epub at all").unwrap();

    run(&library);

    let (key, scanned_at) = edition_row(&library, &id);
    assert_eq!(key, None);
    assert!(scanned_at.is_some(), "an unreadable file is still scanned");
    assert_eq!(pending_count(&library), 0, "and is never retried");

    // A second pass has nothing to do and changes nothing.
    run(&library);
    assert_eq!(edition_row(&library, &id), (None, scanned_at));
}

#[test]
fn a_tombstone_is_never_scanned() {
    let (_dir, library, id) = unscanned_book(UUID);
    library.remove(&id).unwrap();

    run(&library);

    let (key, scanned_at) = edition_row(&library, &id);
    assert_eq!(
        key, None,
        "a tombstone has no file to read an identity from"
    );
    assert_eq!(
        scanned_at, None,
        "and is not stamped either — it is simply never on the list"
    );
}

/// The liveness recheck: `removed_at IS NULL` held when the pending
/// snapshot was taken, and a `remove` landed before the write transaction
/// opened. The book must be left alone rather than written to and stamped.
#[test]
fn a_book_removed_mid_pass_is_left_alone() {
    let (_dir, library, id) = unscanned_book(UUID);
    let file_path = format!("books/{id}.epub");
    library.remove(&id).unwrap();

    // Straight at the per-book step, as the pass reaches it holding a
    // snapshot taken while the book was still live.
    let mut conn = crate::core::db::open_connection(&library.data_dir.join("inkuna.db")).unwrap();
    super::backfill_book(&mut conn, &library.data_dir, &id, &file_path).unwrap();

    assert_eq!(edition_row(&library, &id), (None, None));
}

/// Why the pass exists at all. A library already installed carries books
/// whose `edition_key` the V12 migration could not fill; if it stayed NULL
/// forever, a book finished before the upgrade and a copy of the same
/// edition imported after it would count as two forever — exactly the
/// double count the column was added to close.
#[test]
fn a_backfilled_book_merges_with_a_later_import_of_its_edition() {
    let (dir, library, old_id) = unscanned_book(UUID);
    // A second, differently-encoded copy of the same edition, imported
    // after the upgrade — so it is keyed at import.
    let second = dir.path().join("second.epub");
    write_identified(&second, UUID, "再版の本文、別のバイト列");
    let new_id = match library.import(second.to_str().unwrap()).unwrap() {
        ImportOutcome::Imported(p) => p.id,
        other => panic!("unexpected {other:?}"),
    };
    assert_ne!(new_id, old_id, "a genuinely new row, not a restore");
    for id in [&old_id, &new_id] {
        let conn = library.writer.lock().unwrap();
        conn.execute(
            "UPDATE publications SET finished_at = ?1 WHERE id = ?2",
            rusqlite::params![crate::core::time::unix_now(), id],
        )
        .unwrap();
    }
    assert_eq!(
        library
            .stats_overview("UTC", chrono::Weekday::Mon)
            .unwrap()
            .books_finished_this_year,
        2,
        "unkeyed, the old book is still its own edition"
    );

    run(&library);

    assert_eq!(
        library
            .stats_overview("UTC", chrono::Weekday::Mon)
            .unwrap()
            .books_finished_this_year,
        1,
        "backfilled, the two copies are one edition finished once"
    );
}
