use inkuna_engine::Coordinate;

use crate::test_support::write_epub;
use crate::{CoreError, ImportOutcome, Library};

fn library_with_book() -> (tempfile::TempDir, Library, String) {
    let dir = tempfile::tempdir().unwrap();
    let epub = dir.path().join("book.epub");
    write_epub(&epub, "月光書房", "紫式部", "ja");
    let library = Library::open(dir.path().join("library")).unwrap();
    let id = match library.import(epub.to_str().unwrap()).unwrap() {
        ImportOutcome::Imported(p) => p.id,
        other => panic!("unexpected {other:?}"),
    };
    (dir, library, id)
}

fn at(spine_idx: u32, char_offset: u64) -> Coordinate {
    Coordinate {
        spine_idx,
        char_offset,
    }
}

/// Replaces the book's synthetic position rows with a known breakdown.
fn seed_positions(library: &Library, id: &str, counts: &[u32]) {
    let conn = library.writer.lock().unwrap();
    conn.execute(
        "DELETE FROM resource_positions WHERE publication_id = ?1",
        [id],
    )
    .unwrap();
    let mut start: i64 = 1;
    for (spine_idx, &count) in counts.iter().enumerate() {
        conn.execute(
            "INSERT INTO resource_positions
                (publication_id, spine_idx, start_position, position_count)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, spine_idx as i64, start, count],
        )
        .unwrap();
        start += i64::from(count);
    }
}

#[test]
fn progress_roundtrip_coordinate() {
    let (_dir, library, id) = library_with_book();

    library
        .update_progress(&id, at(1, 42), 0.42, Some(12))
        .unwrap();

    let publication = library.publication(&id).unwrap();
    assert_eq!(publication.coordinate, Some(at(1, 42)));
    assert_eq!(publication.progression, 0.42);
    assert!(publication.last_opened_at.is_some());
    assert!(publication.finished_at.is_none());

    assert!(matches!(
        library.update_progress("missing", at(0, 0), 0.5, None),
        Err(CoreError::NotFound(_))
    ));
}

#[test]
fn publication_coordinate_none_until_written() {
    let (_dir, library, id) = library_with_book();
    assert_eq!(library.publication(&id).unwrap().coordinate, None);

    library.update_progress(&id, at(0, 7), 0.1, None).unwrap();
    assert_eq!(library.publication(&id).unwrap().coordinate, Some(at(0, 7)));
}

#[test]
fn position_derived_from_coordinate() {
    let (_dir, library, id) = library_with_book();
    seed_positions(&library, &id, &[10, 5]);

    // Mid chapter 2 (spine 1): start 11 + 3000/1024 (= 2) = position 13.
    assert_eq!(library.position_of(&id, at(1, 3000)).unwrap(), 13);
    assert_eq!(library.position_count(&id).unwrap(), 15);
    // Past-end coordinates clamp to the resource's / book's last position.
    assert_eq!(library.position_of(&id, at(1, 999_999)).unwrap(), 15);
    assert_eq!(library.position_of(&id, at(9, 0)).unwrap(), 15);
    assert!(matches!(
        library.position_of("missing", at(0, 0)),
        Err(CoreError::NotFound(_))
    ));
    assert!(matches!(
        library.position_count("missing"),
        Err(CoreError::NotFound(_))
    ));

    // A progress write with `position: None` derives the same value into
    // the open session's end_position.
    let session_id = library.session_start(&id).unwrap();
    library
        .update_progress(&id, at(1, 3000), 0.9, None)
        .unwrap();
    let end_position: Option<i64> = library
        .readers
        .with(|conn| {
            conn.query_row(
                "SELECT end_position FROM sessions WHERE id = ?1",
                [&session_id],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .unwrap();
    assert_eq!(end_position, Some(13));
}

#[test]
fn stub_coordinate_uses_progression_for_session_position() {
    let (_dir, library, id) = library_with_book();
    seed_positions(&library, &id, &[10, 5]);
    let session_id = library.session_start(&id).unwrap();

    // Plan-01 shells report this placeholder coordinate until the reader
    // engine is wired through. It must not make a 90%-through sitting look
    // like it never left synthetic position one.
    library.update_progress(&id, at(0, 0), 0.9, None).unwrap();

    let end_position: Option<i64> = library
        .readers
        .with(|conn| {
            conn.query_row(
                "SELECT end_position FROM sessions WHERE id = ?1",
                [&session_id],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .unwrap();
    assert_eq!(end_position, Some(13));
}

#[test]
fn bookmark_defaults_before_reconcile() {
    let (_dir, library, id) = library_with_book();
    // A legacy row the rebaseline has not converted: coordinate columns
    // NULL.
    {
        let conn = library.writer.lock().unwrap();
        conn.execute(
            "INSERT INTO bookmarks (id, publication_id, locator, progression, created_at)
             VALUES ('legacy', ?1, '{}', 0.5, 100)",
            [&id],
        )
        .unwrap();
    }
    let listed = library.bookmarks(&id).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].coordinate, at(0, 0));
}

#[test]
fn auto_finish_fires_only_on_upward_crossing() {
    let (_dir, library, id) = library_with_book();
    let origin = at(0, 0);

    library.update_progress(&id, origin, 0.9, None).unwrap();
    assert!(library.publication(&id).unwrap().finished_at.is_none());

    // Crossing the threshold from below finishes.
    library.update_progress(&id, origin, 0.997, None).unwrap();
    assert!(library.publication(&id).unwrap().finished_at.is_some());

    // Explicit unfinish sticks while staying at the end of the book.
    library.set_finished(&id, false).unwrap();
    library.update_progress(&id, origin, 0.998, None).unwrap();
    library.update_progress(&id, origin, 1.0, None).unwrap();
    assert!(library.publication(&id).unwrap().finished_at.is_none());

    // Leaving the end and re-reaching it crosses upward again.
    library.update_progress(&id, origin, 0.5, None).unwrap();
    library.update_progress(&id, origin, 1.0, None).unwrap();
    assert!(library.publication(&id).unwrap().finished_at.is_some());

    // Explicit finish/unfinish round-trips.
    library.set_finished(&id, false).unwrap();
    assert!(library.publication(&id).unwrap().finished_at.is_none());
    library.set_finished(&id, true).unwrap();
    assert!(library.publication(&id).unwrap().finished_at.is_some());
}
