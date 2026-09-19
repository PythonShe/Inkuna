//! Removal is a tombstone, not a delete: the disk is freed and the book
//! disappears from every library read, but the reading history hanging off
//! its row survives for a later re-import to reclaim.

use super::*;
use crate::test_support::{imported, write_epub};
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
    library
        .add_bookmark(
            &id,
            Some(Coordinate {
                spine_idx: 0,
                char_offset: 3,
            }),
            0.1,
        )
        .unwrap();
    library.set_finished(&id, true).unwrap();

    Fixture {
        data_dir,
        library,
        file_path: publication.file_path,
        cover_path: publication.cover_path.unwrap(),
        id,
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
                 FROM publications WHERE id = ?1",
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
