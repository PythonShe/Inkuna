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

/// A tombstone whose identity was never derived is the one row this pass
/// must leave strictly alone: it has no file to read, so it would be
/// re-opened on every launch forever, and V11's freeze trigger would drop
/// the stamp that stops the retry.
///
/// Since `remove` now derives the identity before it tombstones, the only
/// way such a row still arises is a shipped v10 binary's delete — its
/// `DELETE FROM publications` becomes a tombstone through
/// `publications_soft_delete`, which knows nothing of V12's columns. That
/// is the row built here.
#[test]
fn an_unscanned_tombstone_is_never_scanned() {
    let (_dir, library, id) = unscanned_book(UUID);
    {
        let conn = library.writer.lock().unwrap();
        // v10-view-sql: v10's own `Library::remove`, verbatim.
        conn.execute("DELETE FROM publications WHERE id = ?1", [&id])
            .unwrap();
    }
    assert_eq!(
        edition_row(&library, &id),
        (None, None),
        "the premise: a tombstone that never had its identity derived"
    );

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
    assert_eq!(pending_count(&library), 0);
}

/// The liveness recheck: `removed_at IS NULL` held when the pending
/// snapshot was taken, and a removal landed before the write transaction
/// opened. The book must be left alone rather than written to and stamped.
///
/// The removal is v10's, for the same reason as above — a `Library::remove`
/// leaves the row stamped, so the `edition_scanned_at` half of the gate
/// would answer first and the liveness half would never be reached. Here
/// only liveness can account for the result, and the book's file is still
/// on disk, so nothing but the gate stands between the pass and the write.
#[test]
fn a_book_removed_mid_pass_is_left_alone() {
    let (_dir, library, id) = unscanned_book(UUID);
    let file_path = format!("books/{id}.epub");
    {
        let conn = library.writer.lock().unwrap();
        // v10-view-sql: v10's own `Library::remove`, verbatim.
        conn.execute("DELETE FROM publications WHERE id = ?1", [&id])
            .unwrap();
    }
    assert!(
        library.data_dir.join(&file_path).exists(),
        "the premise: a readable file, so only the gate can refuse the write"
    );

    // Straight at the per-book step, as the pass reaches it holding a
    // snapshot taken while the book was still live.
    let mut conn = crate::core::db::open_connection(&library.data_dir.join("inkuna.db")).unwrap();
    super::backfill_book(&mut conn, &library.data_dir, &id, &file_path).unwrap();

    assert_eq!(edition_row(&library, &id), (None, None));
}

/// Finding 2's case, end to end through the stat itself. A pre-V12 book
/// removed before the pass reaches it is the one book that could never be
/// keyed afterwards: the file is gone and the worklist skips tombstones,
/// while the tombstone keeps counting in `books_finished_this_year`
/// (deliberately — a book you finished and deleted is still a book you
/// finished). A differently encoded copy of the same edition would then
/// count beside it, which is exactly the double count V12 closes.
///
/// So `remove` derives the identity while the file is still there, in the
/// same statement that makes the row a tombstone — the last statement that
/// can, because V11's freeze trigger drops every later write.
#[test]
fn a_book_removed_before_the_pass_still_merges_with_its_edition() {
    let (dir, library, old_id) = unscanned_book(UUID);
    {
        let conn = library.writer.lock().unwrap();
        conn.execute(
            "UPDATE publications_all SET finished_at = ?1 WHERE id = ?2",
            rusqlite::params![crate::core::time::unix_now(), &old_id],
        )
        .unwrap();
    }

    // Removed while still unscanned — the pass never got to it.
    assert_eq!(pending_count(&library), 1);
    library.remove(&old_id).unwrap();
    let (key, scanned_at) = edition_row(&library, &old_id);
    assert_eq!(key.as_deref(), Some(KEY), "the removal derived the identity");
    assert!(scanned_at.is_some(), "and retired the row from the pass");

    // A differently-encoded copy of the same edition, imported and
    // finished afterwards.
    let second = dir.path().join("second.epub");
    write_identified(&second, UUID, "再版の本文、別のバイト列");
    let new_id = match library.import(second.to_str().unwrap()).unwrap() {
        ImportOutcome::Imported(p) => p.id,
        other => panic!("unexpected {other:?}"),
    };
    assert_ne!(new_id, old_id, "a genuinely new row, not a restore");
    {
        let conn = library.writer.lock().unwrap();
        conn.execute(
            "UPDATE publications_all SET finished_at = ?1 WHERE id = ?2",
            rusqlite::params![crate::core::time::unix_now(), &new_id],
        )
        .unwrap();
    }

    // The pass runs and cannot help: the tombstone is not on its worklist.
    run(&library);

    assert_eq!(
        library
            .stats_overview("UTC", chrono::Weekday::Mon)
            .unwrap()
            .books_finished_this_year,
        1,
        "one edition, finished once — the tombstone carries the key it was \
         given at removal"
    );
}

/// The removal is not allowed to *overwrite* an identity, only to finish a
/// missing one: a book whose file no longer parses would otherwise have
/// the key import found replaced by the `None` the removal read.
#[test]
fn a_removal_keeps_the_key_the_import_already_found() {
    let (_dir, library, id) = unscanned_book(UUID);
    run(&library);
    let scanned = edition_row(&library, &id);
    assert_eq!(scanned.0.as_deref(), Some(KEY));

    // The file rots between the scan and the removal.
    std::fs::write(library.data_dir.join(format!("books/{id}.epub")), b"junk").unwrap();
    library.remove(&id).unwrap();

    assert_eq!(
        edition_row(&library, &id),
        scanned,
        "the removal left the stamped identity exactly as it found it"
    );
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
            "UPDATE publications_all SET finished_at = ?1 WHERE id = ?2",
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

/// Finding 1's window. The metadata read happens outside the transaction,
/// so a remove + re-import can land inside it — and the re-import's
/// `revive` already wrote the real `edition_key` and stamped
/// `edition_scanned_at`. The row reads live again, so a liveness-only
/// recheck would let this pass overwrite that key with the `None` its
/// failed read produced, permanently: the stamp blocks every retry.
#[test]
fn a_book_restored_mid_pass_keeps_the_key_its_restore_wrote() {
    let (dir, library, id) = unscanned_book(UUID);
    let file_path = format!("books/{id}.epub");
    // The source the re-import re-reads: `remove` deletes the stored copy,
    // and the same bytes are what match the tombstone on its content hash.
    let source = dir.path().join("book.epub");

    // The remove lands first, so the pass's read — which runs outside every
    // transaction — finds no file and produces no identity.
    library.remove(&id).unwrap();
    let key = super::read_identity(&library.data_dir, &id, &file_path);
    assert_eq!(key, None, "the file was gone when the pass read it");

    // Then the re-import, still inside the pass's window: `revive` writes
    // the real key off the arriving file and stamps the row scanned.
    match library.import(source.to_str().unwrap()).unwrap() {
        ImportOutcome::Restored { publication, .. } => assert_eq!(publication.id, id),
        other => panic!("unexpected {other:?}"),
    }
    let restored = edition_row(&library, &id);
    assert_eq!(restored.0.as_deref(), Some(KEY), "the restore keyed it");
    assert!(restored.1.is_some(), "and stamped it scanned");

    // Only now does the pass reach its write transaction, holding the
    // snapshot it took while the book was live and unscanned.
    let mut conn = crate::core::db::open_connection(&library.data_dir.join("inkuna.db")).unwrap();
    super::write_identity(&mut conn, &id, key).unwrap();

    assert_eq!(
        edition_row(&library, &id),
        restored,
        "the row is no longer the pass's to write"
    );
}

/// Finding 2. `edition_scanned_at` retires a row from the only pass that
/// could ever fill its merge key, so the repair must write the key whole:
/// the stat merges on `edition_key` AND `title_key`, and a row stamped
/// with a NULL `title_key` could never merge again.
#[test]
fn the_repair_writes_the_whole_merge_key() {
    let (_dir, library, id) = unscanned_book(UUID);
    {
        let conn = library.writer.lock().unwrap();
        conn.execute(
            "UPDATE publications_all SET title_key = NULL WHERE id = ?1",
            [&id],
        )
        .unwrap();
    }

    run(&library);

    let (key, title_key, scanned_at): (Option<String>, Option<String>, Option<i64>) = library
        .readers
        .with(|conn| {
            conn.query_row(
                "SELECT edition_key, title_key, edition_scanned_at
                   FROM publications_all WHERE id = ?1",
                [&id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(Into::into)
        })
        .unwrap();
    assert_eq!(key.as_deref(), Some(KEY));
    assert_eq!(
        title_key.as_deref(),
        Some("月光書房"),
        "the stamp must never retire a row with half a merge key"
    );
    assert!(scanned_at.is_some());
}

/// V11's compatibility view is a v10-shaped stand-in, and a v10 binary
/// writing through it knows nothing about `title_key`: its INSERT leaves
/// the column NULL, so the row's backfilled `edition_key` could never
/// merge with anything — the stat needs both halves. The view's trigger
/// cannot fill it either (NFKC + case fold + whitespace strip is not
/// SQLite SQL), so the repair on this side of the boundary is what has to
/// close it, with no cooperation from the old binary at all.
#[test]
fn a_v10_insert_through_the_view_gains_a_title_key() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("library");
    {
        let library = Library::open(&data_dir).unwrap();
        library.search.wait_for_reconcile();
        let conn = library.writer.lock().unwrap();
        // v10-view-sql: exactly what a shipped v10 build writes.
        conn.execute(
            "INSERT INTO publications
                (id, title, authors, format, file_path, content_hash, added_at, progression)
             VALUES ('v10', '　ＭＯＯＮ　書房　', '紫式部', 'epub', 'books/v10.epub',
                     'hash-v10', 100, 0)",
            [],
        )
        .unwrap();
    }

    // A current build opens the library; the pass runs off the open path.
    let library = Library::open(&data_dir).unwrap();
    library.search.wait_for_reconcile();
    assert_eq!(
        stored_title_key(&library, "v10").as_deref(),
        Some("moon書房"),
        "a row a v10 binary inserted must not keep a NULL title_key"
    );
}

/// The other half: a v10 binary's UPDATE through the view rewrites `title`
/// and leaves `title_key` describing the title before it — the desync
/// `import/restore.rs` documents must not happen, arriving from the one
/// writer that cannot be taught otherwise. The row is already stamped, so
/// the `edition_key` pass has retired it and only the repair can see it.
#[test]
fn a_v10_retitle_through_the_view_does_not_leave_a_stale_title_key() {
    let (_dir, library, id) = unscanned_book(UUID);
    run(&library);
    assert_eq!(stored_title_key(&library, &id).as_deref(), Some("月光書房"));
    let (_, scanned_at) = edition_row(&library, &id);
    assert!(scanned_at.is_some(), "the row is retired from the key pass");

    {
        let conn = library.writer.lock().unwrap();
        // v10-view-sql: a v10 retitle, which writes no `title_key`.
        conn.execute(
            "UPDATE publications SET title = '新編　月光書房' WHERE id = ?1",
            [&id],
        )
        .unwrap();
    }
    assert_eq!(
        stored_title_key(&library, &id).as_deref(),
        Some("月光書房"),
        "the fixture must actually be stale before the repair runs"
    );

    run(&library);

    assert_eq!(
        stored_title_key(&library, &id).as_deref(),
        Some("新編月光書房"),
        "a title changed through the view must not leave its key behind"
    );
    // The repair touches the title key alone: the identity the OPF pass
    // already parsed stays, and the row stays retired from that pass.
    let (key, still_scanned) = edition_row(&library, &id);
    assert_eq!(key.as_deref(), Some(KEY));
    assert_eq!(still_scanned, scanned_at);
}

/// Tombstones are the rest of the pass's rule, and the repair keeps it:
/// V11 freezes them, their `edition_key` is NULL forever, and a frozen row
/// silently swallowing the write would make the repair look like it ran.
#[test]
fn the_repair_leaves_tombstones_alone() {
    let (_dir, library, id) = unscanned_book(UUID);
    {
        let conn = library.writer.lock().unwrap();
        conn.execute(
            "UPDATE publications_all
                SET removed_at = 500, title = 'Retitled', title_key = NULL
              WHERE id = ?1",
            [&id],
        )
        .unwrap();
    }

    run(&library);

    assert_eq!(stored_title_key(&library, &id), None);
}

fn stored_title_key(library: &Library, id: &str) -> Option<String> {
    library
        .readers
        .with(|conn| {
            conn.query_row(
                "SELECT title_key FROM publications_all WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .unwrap()
}
