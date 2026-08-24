use crate::test_support::write_epub;
use crate::{ChapterPositionRange, CoreError, ImportOutcome, Library};

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

/// Seeds `resource_positions` (and the matching `position_count`)
/// directly — the shape the import pipeline and reconcile pass write —
/// since the shell-reporting APIs are gone.
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
    if !counts.is_empty() {
        conn.execute(
            "UPDATE publications SET position_count = ?1 WHERE id = ?2",
            rusqlite::params![start - 1, id],
        )
        .unwrap();
    }
}

#[test]
fn missing_id_is_not_found() {
    let (_dir, library, _id) = library_with_book();
    assert!(matches!(
        library.chapter_position_ranges("missing"),
        Err(CoreError::NotFound(_))
    ));
}

#[test]
fn seeded_ranges_derive_chapter_spans() {
    let (_dir, library, id) = library_with_book();

    // Fixture spine: ch01, ch02. Nav TOC: 第一章→ch01, 第一節→ch01#s1
    // (fragment sibling), 第二章→ch02.
    seed_positions(&library, &id, &[10, 5]);

    assert_eq!(
        library.chapter_position_ranges(&id).unwrap(),
        vec![
            // 第一章 spans its resource up to the next chapter's resource.
            ChapterPositionRange {
                chapter_idx: 0,
                start_position: 1,
                end_position: 10
            },
            // Fragment-anchored sibling shares the whole resource span.
            ChapterPositionRange {
                chapter_idx: 1,
                start_position: 1,
                end_position: 10
            },
            // Last chapter runs to the last known position.
            ChapterPositionRange {
                chapter_idx: 2,
                start_position: 11,
                end_position: 15
            },
        ]
    );
    // The total keeps "page N of M" in agreement with the breakdown.
    assert_eq!(library.publication(&id).unwrap().position_count, Some(15));
}

#[test]
fn chapter_ranges_follow_spine_order_when_toc_is_shuffled() {
    let (_dir, library, id) = library_with_book();
    seed_positions(&library, &id, &[10, 5]);
    {
        let conn = library.writer.lock().unwrap();
        conn.execute(
            "UPDATE chapters SET idx = CASE
                WHEN href = 'OEBPS/text/ch02.xhtml' THEN 0
                WHEN href = 'OEBPS/text/ch01.xhtml' THEN 1
                ELSE 2
             END WHERE publication_id = ?1",
            [&id],
        )
        .unwrap();
    }

    assert_eq!(
        library.chapter_position_ranges(&id).unwrap(),
        vec![
            ChapterPositionRange {
                chapter_idx: 0,
                start_position: 11,
                end_position: 15
            },
            ChapterPositionRange {
                chapter_idx: 1,
                start_position: 1,
                end_position: 10
            },
            ChapterPositionRange {
                chapter_idx: 2,
                start_position: 1,
                end_position: 10
            },
        ]
    );
}

#[test]
fn cjk_multi_resource_positions_round_trip_through_coordinates() {
    let (_dir, library, id) = library_with_book();

    // The CJK EPUB fixture has ch01 and ch02. These are the same
    // `resource_positions` rows the import/reconcile pipeline persists.
    seed_positions(&library, &id, &[2, 3]);

    for position in 1..=library.position_count(&id).unwrap() {
        let coordinate = library.coordinate_at_position(&id, position).unwrap();
        assert_eq!(library.position_of(&id, coordinate).unwrap(), position);
    }

    assert_eq!(
        library.coordinate_at_position(&id, 1).unwrap(),
        inkuna_engine::Coordinate {
            spine_idx: 0,
            char_offset: 0,
        }
    );
    assert_eq!(
        library.coordinate_at_position(&id, 2).unwrap(),
        inkuna_engine::Coordinate {
            spine_idx: 0,
            char_offset: 1024,
        }
    );
    assert_eq!(
        library.coordinate_at_position(&id, 3).unwrap(),
        inkuna_engine::Coordinate {
            spine_idx: 1,
            char_offset: 0,
        }
    );
    assert_eq!(
        library.coordinate_at_position(&id, 5).unwrap(),
        inkuna_engine::Coordinate {
            spine_idx: 1,
            char_offset: 2048,
        }
    );
    assert_eq!(
        library.coordinate_at_position(&id, 0).unwrap(),
        library.coordinate_at_position(&id, 1).unwrap()
    );
    assert_eq!(
        library.coordinate_at_position(&id, 6).unwrap(),
        library.coordinate_at_position(&id, 5).unwrap()
    );
}
