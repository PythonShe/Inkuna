//! Removal is a tombstone, not a delete: the disk is freed and the book
//! disappears from every library read, but the reading history hanging off
//! its row survives for a later re-import to reclaim.

use super::*;
use crate::test_support::{imported, restored, write_epub};
use crate::{CoreError, Shelf, Sort, Weekday};
use inkuna_engine::Coordinate;

/// A library holding one imported CJK book, with a reading position, a
/// bookmark, a closed session, and a finished stamp on it — i.e. every
/// kind of history a removal must preserve.
struct Fixture {
    _dir: tempfile::TempDir,
    data_dir: PathBuf,
    library: Library,
    id: String,
    file_path: String,
    cover_path: String,
    /// The source file the fixture imported, kept so a test can re-import
    /// the same book and watch its history come back.
    source: PathBuf,
    bookmark_id: String,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let epub = dir.path().join("book.epub");
    write_epub(&epub, "月光書房", "紫式部", "ja");
    let data_dir = dir.path().join("library");
    let library = Library::open(&data_dir).unwrap();
    let publication = imported(library.import(epub.to_str().unwrap()).unwrap());
    let id = publication.id.clone();

    let session = library.session_start(&id).unwrap();
    library
        .update_progress(
            &id,
            Some(Coordinate {
                spine_idx: 1,
                char_offset: 7,
            }),
            0.42,
            None,
        )
        .unwrap();
    library.session_end(&session).unwrap();
    let bookmark_id = library
        .add_bookmark(
            &id,
            Some(Coordinate {
                spine_idx: 0,
                char_offset: 3,
            }),
            0.1,
        )
        .unwrap()
        .id;
    library.set_finished(&id, true).unwrap();

    Fixture {
        data_dir,
        library,
        file_path: publication.file_path,
        cover_path: publication.cover_path.unwrap(),
        id,
        source: epub,
        bookmark_id,
        _dir: dir,
    }
}

fn count(library: &Library, sql: &str, id: &str) -> i64 {
    library
        .readers
        .with(|conn| {
            conn.query_row(sql, [id], |row| row.get(0))
                .map_err(Into::into)
        })
        .unwrap()
}

fn rows_for(library: &Library, table: &str, id: &str) -> i64 {
    count(
        library,
        &format!("SELECT COUNT(*) FROM {table} WHERE publication_id = ?1"),
        id,
    )
}

#[test]
fn remove_keeps_history_drops_derived_rows_and_frees_disk() {
    let f = fixture();
    let (library, id) = (&f.library, f.id.as_str());

    let fonts = f.data_dir.join(PUBLISHER_FONT_DIR).join(id);
    std::fs::create_dir_all(&fonts).unwrap();
    std::fs::write(fonts.join("embedded.ttf"), b"font bytes").unwrap();

    assert!(rows_for(library, "resources", id) > 0);
    assert!(rows_for(library, "chapters", id) > 0);
    assert!(rows_for(library, "resource_positions", id) > 0);

    library.remove(id).unwrap();

    // Every byte the book occupied is gone.
    assert!(!f.data_dir.join(&f.file_path).exists());
    assert!(!f.data_dir.join(&f.cover_path).exists());
    assert!(!fonts.exists());

    // Derived rows are gone — including `resource_text`, which cascades
    // from `resources` and so can only be counted crate-wide.
    assert_eq!(rows_for(library, "resources", id), 0);
    assert_eq!(rows_for(library, "chapters", id), 0);
    assert_eq!(rows_for(library, "resource_positions", id), 0);
    let texts: i64 = library
        .readers
        .with(|conn| {
            conn.query_row("SELECT COUNT(*) FROM resource_text", [], |row| row.get(0))
                .map_err(Into::into)
        })
        .unwrap();
    assert_eq!(texts, 0, "resource_text cascades from resources");

    // History is untouched.
    assert_eq!(rows_for(library, "sessions", id), 1);
    assert_eq!(rows_for(library, "bookmarks", id), 1);

    // The row itself survives, tombstoned, with its identity and its
    // frozen coordinates intact.
    let tombstone: (
        Option<i64>,
        Option<String>,
        String,
        Option<String>,
        f64,
        Option<u32>,
    ) = library
        .readers
        .with(|conn| {
            conn.query_row(
                "SELECT removed_at, corpus_digest, file_path, cover_path,
                        progression, position_spine_idx
                 FROM publications_all WHERE id = ?1",
                [id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .map_err(Into::into)
        })
        .unwrap();
    assert!(tombstone.0.is_some(), "removed_at stamped");
    // The digest is taken from `resource_text` *before* those rows are
    // deleted — the corpus asserted gone just above — so a stamp here is
    // the whole of what makes the coordinates safe to hand back.
    assert!(
        tombstone.1.is_some_and(|digest| !digest.is_empty()),
        "the corpus the coordinates index was digested before it was dropped"
    );
    assert_eq!(
        tombstone.2, "",
        "file_path blanked (the column is NOT NULL)"
    );
    assert_eq!(tombstone.3, None, "cover_path dropped");
    assert_eq!(tombstone.4, 0.42, "progression preserved");
    assert_eq!(tombstone.5, Some(1), "coordinate preserved");

    // Removing it again is `NotFound`, exactly as a hard delete was.
    assert!(matches!(library.remove(id), Err(CoreError::NotFound(_))));
}

#[test]
fn a_tombstone_is_invisible_to_every_library_read() {
    let f = fixture();
    let (library, id) = (&f.library, f.id.as_str());
    library.remove(id).unwrap();

    for shelf in [
        Shelf::All,
        Shelf::Unfinished,
        Shelf::Reading,
        Shelf::Finished,
    ] {
        assert!(
            library.list(shelf, Sort::RecentlyAdded).unwrap().is_empty(),
            "{shelf:?} must not show a removed book"
        );
        assert!(
            library
                .list(shelf, Sort::RecentlyOpened)
                .unwrap()
                .is_empty()
        );
    }
    assert!(library.search_library("月光").unwrap().is_empty());
    assert!(library.search_library("紫式部").unwrap().is_empty());
    assert!(matches!(
        library.publication(id),
        Err(CoreError::NotFound(_))
    ));
    assert!(matches!(library.chapters(id), Err(CoreError::NotFound(_))));
    assert!(matches!(library.spine(id), Err(CoreError::NotFound(_))));

    // Writes addressed at it fail the same way, so no shell holding a
    // stale id can keep a removed book's state moving.
    assert!(matches!(
        library.session_start(id),
        Err(CoreError::NotFound(_))
    ));
    assert!(matches!(
        library.update_progress(id, None, 0.9, None),
        Err(CoreError::NotFound(_))
    ));
    assert!(matches!(
        library.set_finished(id, false),
        Err(CoreError::NotFound(_))
    ));
    assert!(matches!(
        library.add_bookmark(id, None, 0.5),
        Err(CoreError::NotFound(_))
    ));

    // The bookmark rows survive the removal, but nothing shell-facing can
    // see them — and nothing shell-facing can destroy the history a
    // re-import is meant to hand back.
    assert!(library.bookmarks(id).unwrap().is_empty());
    assert!(matches!(
        library.remove_bookmark(&f.bookmark_id),
        Err(CoreError::NotFound(_))
    ));

    // Re-importing the same file revives the row, and the bookmark the
    // stale shell could not delete is there waiting.
    let (revived, _) = restored(library.import(f.source.to_str().unwrap()).unwrap());
    assert_eq!(revived.id, f.id, "the same row, revived");
    let bookmarks = library.bookmarks(id).unwrap();
    assert_eq!(bookmarks.len(), 1, "the preserved bookmark comes back");
    assert_eq!(bookmarks[0].id, f.bookmark_id);
}

/// The whole reason the row stays in `publications` instead of moving
/// aside: stats read `sessions` with no join, and a finished book stays
/// finished. Time spent reading is not un-spent by a later delete.
#[test]
fn removal_does_not_erase_reading_history_from_stats() {
    let f = fixture();

    // The fixture's session opens and closes inside the same second, and
    // `update_progress` backfills `start_position` from the same value it
    // writes to `end_position` — so as built it contributes 0 pages and 0
    // minutes, and the two assertions below would compare 0 to 0 and guard
    // nothing. Give it a real page span and a real duration. `started_at`
    // is left exactly where it is: it alone decides which day, week, and
    // month the sitting lands in, and moving it would make this test
    // flake at a local calendar boundary.
    {
        let conn = f.library.writer.lock().unwrap();
        let updated = conn
            .execute(
                "UPDATE sessions
                    SET ended_at = started_at + 1200, updated_at = started_at + 1200,
                        start_position = 4, end_position = 37
                  WHERE publication_id = ?1",
                [&f.id],
            )
            .unwrap();
        assert_eq!(updated, 1, "the fixture leaves exactly one session");
    }

    let before = f
        .library
        .stats_overview("Asia/Tokyo", Weekday::Mon)
        .unwrap();
    assert_eq!(before.books_finished_this_year, 1);
    assert_eq!(before.read_days.len(), 1, "today read");
    assert_eq!(before.pages_this_week, 33, "37 − 4, so the assert can bite");
    assert_eq!(before.minutes_this_month, 20, "1200 seconds of reading");

    f.library.remove(&f.id).unwrap();

    let after = f
        .library
        .stats_overview("Asia/Tokyo", Weekday::Mon)
        .unwrap();
    assert_eq!(
        after.books_finished_this_year, 1,
        "a book you finished stays finished after you delete it"
    );
    assert_eq!(after.pages_this_week, before.pages_this_week);
    assert_eq!(after.minutes_this_month, before.minutes_this_month);
    assert_eq!(after.read_days, before.read_days);
}

/// The sweep must not confuse a tombstone with a live book in either
/// direction: a live book's files stay, and a tombstone's publisher-font
/// directory (left by a remove interrupted before `remove_dir_all`) goes.
#[test]
fn sweep_spares_live_files_and_clears_a_tombstones_font_cache() {
    let f = fixture();
    let dir = tempfile::tempdir().unwrap();
    let second_epub = dir.path().join("second.epub");
    write_epub(&second_epub, "生きてる本", "著者", "ja");
    let live = imported(f.library.import(second_epub.to_str().unwrap()).unwrap());

    f.library.remove(&f.id).unwrap();
    // Re-create what an interrupted remove leaves: the row is already a
    // tombstone but its font cache never got deleted.
    let stale = f.data_dir.join(PUBLISHER_FONT_DIR).join(&f.id);
    std::fs::create_dir_all(&stale).unwrap();
    std::fs::write(stale.join("embedded.ttf"), b"stale").unwrap();
    let live_fonts = f.data_dir.join(PUBLISHER_FONT_DIR).join(&live.id);
    std::fs::create_dir_all(&live_fonts).unwrap();
    std::fs::write(live_fonts.join("embedded.ttf"), b"live").unwrap();

    drop(f.library);
    let library = Library::open(&f.data_dir).unwrap();

    assert!(!stale.exists(), "a tombstone's font cache is swept");
    assert!(live_fonts.exists(), "a live book keeps its font cache");
    assert!(f.data_dir.join(&live.file_path).is_file());
    assert!(f.data_dir.join(live.cover_path.as_ref().unwrap()).is_file());
    assert_eq!(
        library.list(Shelf::All, Sort::RecentlyAdded).unwrap().len(),
        1
    );
}

/// Two removals of one book, racing on the same row. `remove` reads the
/// paths it is about to unlink *through* its writer transaction and acts
/// only on a tombstone `UPDATE` that reported the row claimed, so exactly
/// one call owns the deletion and the other finds a tombstone and says
/// `NotFound`. Reading the row off the reader pool before the lock let
/// both calls act on the same live snapshot and both report success —
/// the stale read a concurrent restore could land in the middle of.
///
/// The writer lock is held while both threads start so the race is the
/// real one and not two removals that happened to serialize.
#[test]
fn two_concurrent_removes_claim_the_row_exactly_once() {
    let f = fixture();
    let library = &f.library;
    let id = f.id.as_str();

    let gate = std::sync::Barrier::new(3);
    let held = library.writer.lock().unwrap();
    let (first, second) = std::thread::scope(|scope| {
        let first = scope.spawn(|| {
            gate.wait();
            library.remove(id)
        });
        let second = scope.spawn(|| {
            gate.wait();
            library.remove(id)
        });
        gate.wait();
        // Long enough for both threads to reach the writer lock (or, before
        // the claim, to have read the live row).
        std::thread::sleep(std::time::Duration::from_millis(50));
        drop(held);
        (first.join().unwrap(), second.join().unwrap())
    });

    let loser = match (first, second) {
        (Ok(()), Err(e)) | (Err(e), Ok(())) => e,
        other => panic!("expected exactly one Ok and one NotFound, got {other:?}"),
    };
    match loser {
        CoreError::NotFound(missing) => assert_eq!(missing, f.id),
        other => panic!("expected NotFound, got {other:?}"),
    }
    assert_eq!(
        library.list(Shelf::All, Sort::RecentlyAdded).unwrap().len(),
        0
    );
}

/// There is no downgrade gate — `migrate` returns early when the file is
/// already at or past the target — so a shipped v10 build opens a v11
/// database and runs its own SQL against it. That SQL is hostile to a
/// tombstone: v10's `remove` is a hard `DELETE FROM publications`, which
/// would cascade the sessions and bookmarks a tombstone exists to keep,
/// and v10's rebaseline pass has no `removed_at` filter, so it would
/// consume the legacy `locator` that is a tombstone's only record of
/// where the reader was. Every statement below is v10's own, verbatim from
/// `git show main:…/library/store.rs` and `…/library/rebaseline.rs`.
///
/// Two distinct defenses contain them, and this test pins both separately
/// because each one alone would be enough to make the other look like it
/// worked. The `publications` **view** is what stops v10: it holds no
/// tombstone row at all, so v10's rebaseline UPDATEs match nothing — the
/// freeze trigger is never even reached on that path. The
/// `publications_freeze_tombstone` **trigger** is what stops a v11+ writer,
/// which names `publications_all` directly and so does see the tombstone;
/// the second half below replays the very same v10 statements against that
/// name to reach it.
#[test]
fn a_v10_binary_cannot_destroy_a_tombstone() {
    let f = fixture();
    let db_path = f.data_dir.join("inkuna.db");
    const LEGACY_LOCATOR: &str = r#"{"href":"OEBPS/ch01.xhtml"}"#;

    // Put the book in the state v10's rebaseline pass goes looking for: a
    // legacy locator, no coordinates yet, never reconciled.
    f.library
        .writer
        .lock()
        .unwrap()
        .execute(
            // v10-view-sql: v10's own statement, verbatim.
            "UPDATE publications
                SET locator = ?1, position_spine_idx = NULL,
                    position_char_offset = NULL, reconciled_at = NULL
              WHERE id = ?2",
            rusqlite::params![LEGACY_LOCATOR, &f.id],
        )
        .unwrap();
    let sessions_before = rows_for(&f.library, "sessions", &f.id);
    let bookmarks_before = rows_for(&f.library, "bookmarks", &f.id);
    assert_eq!(sessions_before, 1, "there is history to lose");
    assert_eq!(bookmarks_before, 1);

    // The old build has the database to itself, on a connection set up
    // exactly as it sets one up (`foreign_keys` ON, so a hard delete would
    // really cascade).
    drop(f.library);
    {
        let conn = open_connection(&db_path).unwrap();

        // v10 `Library::remove`.
        // v10-view-sql: v10's own statement, verbatim.
        conn.execute("DELETE FROM publications WHERE id = ?1", [&f.id])
            .unwrap();
        let _ = std::fs::remove_file(f.data_dir.join(&f.file_path));
        let _ = std::fs::remove_file(f.data_dir.join(&f.cover_path));
        let _ = std::fs::remove_dir_all(f.data_dir.join(PUBLISHER_FONT_DIR).join(&f.id));

        // The row survived as a correct tombstone, and so did the history.
        let (removed_at, corpus_digest, file_path, cover_path, locator): (
            Option<i64>,
            Option<String>,
            String,
            Option<String>,
            Option<String>,
        ) = conn
            .query_row(
                "SELECT removed_at, corpus_digest, file_path, cover_path, locator
                 FROM publications_all WHERE id = ?1",
                [&f.id],
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
        assert!(removed_at.is_some(), "the delete became a tombstone");
        assert_eq!(file_path, "", "file_path blanked (the column is NOT NULL)");
        assert_eq!(cover_path, None);
        assert_eq!(
            corpus_digest, None,
            "v10 cannot digest a corpus; NULL degrades exactly like a mismatch"
        );
        assert_eq!(locator.as_deref(), Some(LEGACY_LOCATOR));

        let counted =
            |sql: &str| -> i64 { conn.query_row(sql, [&f.id], |row| row.get(0)).unwrap() };
        assert_eq!(
            counted("SELECT COUNT(*) FROM sessions WHERE publication_id = ?1"),
            sessions_before,
            "the cascade never ran"
        );
        assert_eq!(
            counted("SELECT COUNT(*) FROM bookmarks WHERE publication_id = ?1"),
            bookmarks_before
        );
        assert_eq!(
            counted("SELECT COUNT(*) FROM resources WHERE publication_id = ?1"),
            0,
            "derived rows go, exactly as v11's own remove drops them"
        );
        assert_eq!(
            counted("SELECT COUNT(*) FROM chapters WHERE publication_id = ?1"),
            0
        );
        assert_eq!(
            counted("SELECT COUNT(*) FROM resource_positions WHERE publication_id = ?1"),
            0
        );
        let texts: i64 = conn
            .query_row("SELECT COUNT(*) FROM resource_text", [], |row| row.get(0))
            .unwrap();
        assert_eq!(texts, 0, "resource_text still cascades from resources");

        // v10 `rebaseline::rebaseline_one`, steps 3 and 5: the conversion
        // pair that consumes the locator, then the `reconciled_at` stamp.
        // These name `publications`, so it is the view — not the freeze
        // trigger — that is on trial here: the tombstone is simply not in
        // the rowset these statements can reach.
        let converted = conn
            .execute(
                // v10-view-sql: v10's own statement, verbatim.
                "UPDATE publications
             SET position_spine_idx = ?1, position_char_offset = ?2, locator = NULL
             WHERE id = ?3 AND position_spine_idx IS NULL",
                rusqlite::params![0_i64, 0_i64, &f.id],
            )
            .unwrap();
        assert_eq!(
            converted, 0,
            "the view holds no tombstone row, so v10's conversion matched nothing"
        );
        conn.execute(
            // v10-view-sql: v10's own statement, verbatim.
            "UPDATE publications SET locator = NULL WHERE id = ?1",
            [&f.id],
        )
        .unwrap();
        conn.execute(
            // v10-view-sql: v10's own statement, verbatim.
            "UPDATE publications SET reconciled_at = ?1 WHERE id = ?2",
            rusqlite::params![999_i64, &f.id],
        )
        .unwrap();

        let (locator, reconciled_at): (Option<String>, Option<i64>) = conn
            .query_row(
                "SELECT locator, reconciled_at FROM publications_all WHERE id = ?1",
                [&f.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            locator.as_deref(),
            Some(LEGACY_LOCATOR),
            "the reading position a restore will rebaseline is still there"
        );
        assert_eq!(
            reconciled_at, None,
            "nothing told the tombstone its deleted corpus was canonical"
        );

        // The other defense, on trial on its own. A v11+ pass — a
        // rebaseline or backfill racing a `remove` — names
        // `publications_all`, so the view does not stand between it and the
        // tombstone and `publications_freeze_tombstone` is what has to
        // catch it. Same three statements, same order, only the table name
        // differs, so nothing but the trigger can account for the result.
        let frozen = conn
            .execute(
                "UPDATE publications_all
                    SET position_spine_idx = ?1, position_char_offset = ?2, locator = NULL
                  WHERE id = ?3 AND position_spine_idx IS NULL",
                rusqlite::params![0_i64, 0_i64, &f.id],
            )
            .unwrap();
        assert_eq!(
            frozen, 0,
            "the row was matched, and the freeze trigger swallowed the write"
        );
        assert_eq!(
            conn.execute(
                "UPDATE publications_all SET locator = NULL WHERE id = ?1",
                [&f.id],
            )
            .unwrap(),
            0,
            "and the unconditional locator clear too"
        );
        assert_eq!(
            conn.execute(
                "UPDATE publications_all SET reconciled_at = ?1 WHERE id = ?2",
                rusqlite::params![999_i64, &f.id],
            )
            .unwrap(),
            0,
            "and the reconciled_at stamp"
        );

        let (locator, reconciled_at, still_removed): (Option<String>, Option<i64>, Option<i64>) =
            conn.query_row(
                "SELECT locator, reconciled_at, removed_at FROM publications_all WHERE id = ?1",
                [&f.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            locator.as_deref(),
            Some(LEGACY_LOCATOR),
            "the trigger left the position untouched"
        );
        assert_eq!(reconciled_at, None);
        assert!(still_removed.is_some(), "and it is still a tombstone");

        // The freeze is not a blanket write ban: a restore identifies
        // itself by clearing `removed_at` in the same statement, and that
        // one statement does land. Without this, a trigger that ignored
        // *every* update would pass the assertions above just as well.
        assert_eq!(
            conn.execute(
                "UPDATE publications_all SET removed_at = NULL, reconciled_at = ?1 WHERE id = ?2",
                rusqlite::params![999_i64, &f.id],
            )
            .unwrap(),
            1,
            "restore is the trigger's one exception"
        );
        conn.execute(
            "UPDATE publications_all SET removed_at = ?1, reconciled_at = NULL WHERE id = ?2",
            rusqlite::params![1_000_i64, &f.id],
        )
        .unwrap();
    }

    // And v11 picks the tombstone back up: the same bytes restore onto it,
    // with coordinates degraded because v10 left no digest behind.
    let library = Library::open(&f.data_dir).unwrap();
    let (publication, coordinates_restored) =
        restored(library.import(f.source.to_str().unwrap()).unwrap());
    assert_eq!(publication.id, f.id);
    assert!(
        !coordinates_restored,
        "unknown provenance degrades like a mismatch"
    );
    assert_eq!(rows_for(&library, "sessions", &f.id), sessions_before);
    assert_eq!(
        count(
            &library,
            "SELECT COUNT(*) FROM bookmarks WHERE id = ?1",
            &f.bookmark_id
        ),
        1,
        "the bookmark v10 would have cascaded away came back with the book"
    );
}

/// Everything below is about the v10 *experience* rather than v10
/// containment: K1-A already made the old binary harmless, and these are
/// the tests that it is also unembarrassing. `publications` is a view of
/// the live rows, so a shipped v10 build on a v11+ database behaves
/// indistinguishably from one on a v10 database.
///
/// Every statement marked "v10's own" is verbatim from
/// `git show main:…/library/queries.rs` and `…/import/pipeline.rs`, with
/// `PUB_COLUMNS` expanded — v10 built these by `format!`, and its column
/// list is byte-identical to today's.
const V10_PUB_COLUMNS: &str = "id, title, authors, language, text_encoding, format, file_path, \
     cover_path, added_at, progression, position_spine_idx, position_char_offset, \
     position_count, finished_at, last_opened_at";

/// v10's `Library::insert_publication`, verbatim.
// v10-view-sql: v10's own statement, verbatim.
const V10_INSERT: &str = "INSERT INTO publications
                    (id, title, authors, language, text_encoding, format, file_path,
                     cover_path, content_hash, added_at, progression, reconciled_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)";

fn stored<T: rusqlite::types::FromSql>(library: &Library, column: &str, id: &str) -> Option<T> {
    library
        .readers
        .with(|conn| {
            conn.query_row(
                &format!("SELECT {column} FROM publications_all WHERE id = ?1"),
                [id],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .unwrap()
}

/// v10's `Shelf::All` filter is the empty string — it was written when
/// every row was a live book — so nothing inside the old binary can keep a
/// tombstone off its shelves; before the view, one showed up as a titled
/// row with no file behind it. The view is the whole of the fix, and it is
/// why the view must expose a column literally named `rowid`: v10's tie
/// break orders by it.
#[test]
fn a_v10_list_query_cannot_see_a_tombstone() {
    let f = fixture();
    let other = f.source.parent().unwrap().join("other.epub");
    write_epub(&other, "Second Book", "Author", "en");
    let live = imported(f.library.import(other.to_str().unwrap()).unwrap()).id;
    f.library.remove(&f.id).unwrap();

    drop(f.library);
    let conn = open_connection(&f.data_dir.join("inkuna.db")).unwrap();
    let listed: Vec<String> = {
        // v10 `Library::list(Shelf::All, Sort::RecentlyAdded)`, verbatim.
        let sql = format!(
            // v10-view-sql: v10's own statement, verbatim.
            "SELECT {V10_PUB_COLUMNS} FROM publications  ORDER BY added_at DESC, rowid DESC"
        );
        let mut stmt = conn.prepare(&sql).unwrap();
        let rows = stmt.query_map([], |row| row.get::<_, String>(0)).unwrap();
        rows.collect::<Result<_, _>>().unwrap()
    };
    assert_eq!(
        listed,
        vec![live],
        "the removed book is not a row v10 can reach"
    );

    // And the other three shelves, whose filters v10 wrote assuming the
    // same thing.
    for filter in [
        "WHERE last_opened_at IS NOT NULL AND finished_at IS NULL",
        "WHERE finished_at IS NULL",
        "WHERE finished_at IS NOT NULL",
    ] {
        let count: i64 = conn
            .query_row(
                // v10-view-sql: v10's own statement, verbatim.
                &format!("SELECT COUNT(*) FROM publications {filter}"),
                [],
                |row| row.get(0),
            )
            .unwrap();
        let listed_removed: i64 = conn
            .query_row(
                &format!(
                    // v10-view-sql: v10's own statement, verbatim.
                    "SELECT COUNT(*) FROM (SELECT id FROM publications {filter}) WHERE id = ?1"
                ),
                [&f.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(listed_removed, 0, "{filter} surfaced the tombstone");
        assert!(count <= 1);
    }
}

/// The one place the view alone is not enough, and the one place v10 is
/// deliberately *not* given the behaviour it expects. v10's dedupe reads
/// through the view, so the tombstone holding this content is invisible
/// and the import runs on to its INSERT. Letting that INSERT through would
/// mean taking the tombstone out of `UNIQUE(content_hash)`'s way, and the
/// tombstone is not live — `publications_soft_delete` does not fire, the
/// delete is real, and the sessions and bookmarks it exists to keep
/// cascade away with it. `publications_view_insert` aborts instead.
///
/// The whole v10 sequence is replayed verbatim, INSERT and fallback both,
/// because "fails safely" is a claim about what the old binary does next,
/// not just about the row: a constraint violation is the case v10 already
/// handles — it rolls its transaction back, sweeps the file and cover it
/// staged, re-reads the view for the duplicate it assumes it lost to,
/// finds none, and reports the import as failed. Recoverable. The history
/// the alternative destroys is not.
#[test]
fn a_v10_dedupe_reimport_cannot_destroy_a_tombstone() {
    let f = fixture();
    let hash: String = stored(&f.library, "content_hash", &f.id).unwrap();
    let removed_at: i64 = {
        f.library.remove(&f.id).unwrap();
        assert_eq!(rows_for(&f.library, "sessions", &f.id), 1);
        assert_eq!(rows_for(&f.library, "bookmarks", &f.id), 1);
        stored(&f.library, "removed_at", &f.id).unwrap()
    };

    drop(f.library);
    let conn = open_connection(&f.data_dir.join("inkuna.db")).unwrap();

    // v10 `Library::publication_by_hash`, verbatim. It is the check that
    // used to answer "Already in your library" for a file the user could
    // neither open nor re-add.
    // v10-view-sql: v10's own statement, verbatim.
    let sql = format!("SELECT {V10_PUB_COLUMNS} FROM publications WHERE content_hash = ?1");
    let by_hash = |conn: &rusqlite::Connection| -> Option<String> {
        conn.prepare(&sql)
            .unwrap()
            .query_map([&hash], |row| row.get::<_, String>(0))
            .unwrap()
            .next()
            .transpose()
            .unwrap()
    };
    assert_eq!(by_hash(&conn), None, "v10's dedupe misses the tombstone");

    // So v10 proceeds to insert. Same bytes, same hash, a brand-new id —
    // inside a transaction, because that is where v10 runs it.
    let tx = conn.unchecked_transaction().unwrap();
    let refused = tx
        .execute(
            V10_INSERT,
            rusqlite::params![
                "v10-new-id",
                "月光書房",
                "紫式部",
                "ja",
                None::<String>,
                "epub",
                "books/v10-new-id.epub",
                None::<String>,
                &hash,
                1_000_i64,
                0.0_f64,
                1_000_i64,
            ],
        )
        .expect_err("the INSERT must fail rather than clear the tombstone away");

    // v10's `is_constraint_violation` is what routes this to the safe
    // path: it matches on the PRIMARY result code, and RAISE(ABORT)
    // reports SQLITE_CONSTRAINT (extended: SQLITE_CONSTRAINT_TRIGGER, 1811)
    // exactly as the UNIQUE violation v10 was written for. Anything else
    // would take v10's `Err(e) => return Err(e)` arm instead — still no
    // data loss, but surfaced as a database error rather than a failed
    // import.
    match &refused {
        rusqlite::Error::SqliteFailure(err, message) => {
            assert_eq!(err.code, rusqlite::ErrorCode::ConstraintViolation);
            assert_eq!(err.extended_code, 1811, "SQLITE_CONSTRAINT_TRIGGER");
            assert!(
                message
                    .as_deref()
                    .is_some_and(|m| m.contains("removed book")),
                "the abort must say why, got {message:?}"
            );
        }
        other => panic!("expected a constraint violation, got {other:?}"),
    }
    // ABORT backs the statement out, not the transaction: v10 drops its
    // own transaction here and carries on on the same connection.
    drop(tx);
    assert_eq!(
        by_hash(&conn),
        None,
        "v10's post-failure lookup still cannot see the tombstone, so it \
         reports NotFound — a failed import, which the user can retry"
    );

    // The row and every row hanging off it are untouched.
    let (rows, id, still_removed_at): (i64, String, i64) = conn
        .query_row(
            "SELECT COUNT(*), MIN(id), MIN(removed_at) FROM publications_all
              WHERE content_hash = ?1",
            [&hash],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(rows, 1, "no second row, and no lost one either");
    assert_eq!(id, f.id, "the tombstone, not a v10 replacement");
    assert_eq!(still_removed_at, removed_at, "not even re-stamped");
    for table in ["sessions", "bookmarks"] {
        let kept: i64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE publication_id = ?1"),
                [&f.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(kept, 1, "{table} survived the v10 re-import attempt");
    }

    // And the point of surviving: a current build still restores the book
    // onto that tombstone, history and all.
    drop(conn);
    let library = Library::open(&f.data_dir).unwrap();
    let (publication, coordinates_restored) =
        restored(library.import(f.source.to_str().unwrap()).unwrap());
    assert_eq!(publication.id, f.id);
    assert!(coordinates_restored, "the corpus digest still matches");
    assert_eq!(publication.progression, 0.42);
    assert_eq!(rows_for(&library, "sessions", &f.id), 1);
    assert_eq!(
        count(
            &library,
            "SELECT COUNT(*) FROM bookmarks WHERE id = ?1",
            &f.bookmark_id
        ),
        1,
        "the bookmark v10 would have cascaded away is still there"
    );
}

/// How the two trigger layers compose. A v10 `DELETE` now lands on the
/// view's `INSTEAD OF DELETE` first, which forwards to the base table,
/// where K1-A's `BEFORE DELETE` turns it into a tombstone and cancels the
/// row delete with `RAISE(IGNORE)` — abandoning the inner statement alone,
/// so the outer one still reports success to the old binary. And a v10
/// delete aimed at a tombstone matches nothing at all: the view has never
/// heard of that row, so the tombstone cannot be touched a second time.
#[test]
fn a_v10_delete_through_the_view_still_preserves_history() {
    let f = fixture();
    let other = f.source.parent().unwrap().join("other.epub");
    write_epub(&other, "Second Book", "Author", "en");
    let second = imported(f.library.import(other.to_str().unwrap()).unwrap()).id;
    f.library.remove(&f.id).unwrap();
    let frozen_removed_at: i64 = stored(&f.library, "removed_at", &f.id).unwrap();

    drop(f.library);
    let conn = open_connection(&f.data_dir.join("inkuna.db")).unwrap();

    // v10 `Library::remove`, verbatim, over every id it could hold —
    // including the one it cannot see.
    for id in [&f.id, &second] {
        // v10-view-sql: v10's own statement, verbatim.
        conn.execute("DELETE FROM publications WHERE id = ?1", [id])
            .expect("a view with no INSTEAD OF DELETE would refuse this outright");
    }

    let live: i64 = conn
        // v10-view-sql: v10's own statement, verbatim.
        .query_row("SELECT COUNT(*) FROM publications", [], |row| row.get(0))
        .unwrap();
    assert_eq!(live, 0, "both books left the shelf");
    let tombstones: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM publications_all WHERE removed_at IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(tombstones, 2, "neither row was actually deleted");

    // The already-tombstoned row is bit-identical: the delete aimed at it
    // matched no view row, so nothing re-stamped it.
    let restamped: i64 = conn
        .query_row(
            "SELECT removed_at FROM publications_all WHERE id = ?1",
            [&f.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(restamped, frozen_removed_at);

    // And the history the tombstones exist to keep never cascaded.
    for (table, expected) in [("sessions", 1), ("bookmarks", 1)] {
        let kept: i64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE publication_id = ?1"),
                [&f.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(kept, expected, "{table} survived the v10 delete");
    }
}

/// The v11+ side is unmoved by the rename: every read and write names
/// `publications_all` directly, so `remove`, `list` and the restore path
/// behave exactly as they did before the view existed — the view is for
/// the old binary alone and must never become load-bearing for this build.
#[test]
fn v11_remove_revive_and_list_still_work_through_publications_all() {
    let f = fixture();
    let (library, id) = (&f.library, f.id.as_str());

    assert_eq!(
        library.list(Shelf::All, Sort::RecentlyAdded).unwrap().len(),
        1
    );
    library.remove(id).unwrap();

    // Gone from every read, but still a row — on the base table, where a
    // v11 build looks for it.
    assert!(
        library
            .list(Shelf::All, Sort::RecentlyAdded)
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        library.publication(id),
        Err(CoreError::NotFound(_))
    ));
    assert!(stored::<i64>(library, "removed_at", id).is_some());
    let base_rows = count(
        library,
        "SELECT COUNT(*) FROM publications_all WHERE id = ?1",
        id,
    );
    assert_eq!(base_rows, 1, "the tombstone is on the base table");

    // The same bytes come back onto the same row, with its history.
    let (publication, _) = restored(library.import(f.source.to_str().unwrap()).unwrap());
    assert_eq!(publication.id, id);
    assert_eq!(stored::<i64>(library, "removed_at", id), None);
    let shelf = library.list(Shelf::All, Sort::RecentlyAdded).unwrap();
    assert_eq!(shelf.len(), 1);
    assert_eq!(shelf[0].id, id);
    assert_eq!(rows_for(library, "sessions", id), 1);
    assert_eq!(
        count(
            library,
            "SELECT COUNT(*) FROM bookmarks WHERE id = ?1",
            &f.bookmark_id
        ),
        1
    );
}

/// Runs `body` with every pooled reader connection checked out, so any
/// code that reaches for one blocks until this returns.
fn with_pool_drained<T>(pool: &ReaderPool, left: usize, body: &mut dyn FnMut() -> T) -> T {
    if left == 0 {
        return body();
    }
    let mut out = None;
    pool.with(|_| {
        out = Some(with_pool_drained(pool, left - 1, body));
        Ok(())
    })
    .unwrap();
    out.expect("the pool ran the closure")
}

/// `remove` reads the paths it is about to unlink *through its own writer
/// transaction*, not off the reader pool before taking the lock.
///
/// The `claimed != 1` guard underneath it does not cover this. That guard
/// only notices a row that stopped being live; a row whose *paths* moved
/// while it stayed live claims perfectly well, and the unlinks then run
/// against the stale names. That race is real: `cover::backfill` rewrites
/// `cover_path` on a live row under the writer lock and deletes the old
/// file itself, so a `remove` holding a pre-lock read would unlink a name
/// that is already gone and strand the cover the row actually points at.
///
/// Reaching in to time that race would mean pausing production code, so
/// the invariant is pinned at its cause instead: with the reader pool
/// drained to empty, a `remove` that wanted a pooled connection cannot
/// get one, and the whole call has to complete without it. Moving the
/// read back outside the transaction makes this block rather than trip an
/// assertion, which is what the timeout is for. (A pre-lock read on some
/// *fresh* connection would slip past — the reader pool is the only
/// pre-lock read this code has ever had.)
#[test]
fn remove_reads_its_paths_through_its_own_transaction() {
    let f = fixture();
    let book = f.data_dir.join(&f.file_path);
    let cover = f.data_dir.join(&f.cover_path);
    assert!(book.exists() && cover.exists(), "there are files to unlink");

    let (tx, rx) = std::sync::mpsc::channel();
    let library = &f.library;
    let id = f.id.as_str();
    let finished = std::thread::scope(|scope| {
        with_pool_drained(&library.readers, READER_POOL_SIZE, &mut || {
            let tx = tx.clone();
            scope.spawn(move || {
                let _ = tx.send(library.remove(id));
            });
            // On the failing path this waits out the timeout, then lets
            // the pool go so the blocked thread can finish and the scope
            // can join it — the assertion happens after, not here.
            rx.recv_timeout(std::time::Duration::from_secs(10))
        })
    });

    finished
        .expect(
            "remove blocked with the reader pool empty: it is reading the paths \
             it unlinks off a pooled connection before taking the writer lock, \
             not through its own transaction",
        )
        .unwrap();
    assert!(!book.exists(), "the file the row named is gone");
    assert!(!cover.exists(), "and the cover it named");
}
