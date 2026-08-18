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

#[test]
fn ranges_empty_until_reported_and_missing_id_is_not_found() {
    let (_dir, library, id) = library_with_book();

    assert!(library.chapter_position_ranges(&id).unwrap().is_empty());
    assert!(matches!(
        library.chapter_position_ranges("missing"),
        Err(CoreError::NotFound(_))
    ));
    assert!(matches!(
        library.report_position_ranges("missing", &[3]),
        Err(CoreError::NotFound(_))
    ));
}

#[test]
fn report_derives_cumulative_starts_and_chapter_spans() {
    let (_dir, library, id) = library_with_book();

    // Fixture spine: ch01, ch02. Nav TOC: 第一章→ch01, 第一節→ch01#s1
    // (fragment sibling), 第二章→ch02.
    library.report_position_ranges(&id, &[10, 5]).unwrap();

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
fn report_rejects_mismatched_and_zero_resource_counts() {
    let (_dir, library, id) = library_with_book();
    library.report_position_ranges(&id, &[10, 5]).unwrap();

    assert!(matches!(
        library.report_position_ranges(&id, &[10]),
        Err(CoreError::InvalidPositionRanges {
            expected: 2,
            actual: 1,
            has_zero: false,
        })
    ));
    assert!(matches!(
        library.report_position_ranges(&id, &[10, 0]),
        Err(CoreError::InvalidPositionRanges {
            expected: 2,
            actual: 2,
            has_zero: true,
        })
    ));
    assert_eq!(library.publication(&id).unwrap().position_count, Some(15));
    assert_eq!(library.chapter_position_ranges(&id).unwrap().len(), 3);
}

#[test]
fn chapter_ranges_follow_spine_order_when_toc_is_shuffled() {
    let (_dir, library, id) = library_with_book();
    library.report_position_ranges(&id, &[10, 5]).unwrap();
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
fn re_report_replaces_and_empty_report_clears() {
    let (_dir, library, id) = library_with_book();

    library.report_position_ranges(&id, &[10, 5]).unwrap();
    library.report_position_ranges(&id, &[4, 4]).unwrap();
    assert_eq!(
        library
            .chapter_position_ranges(&id)
            .unwrap()
            .last()
            .unwrap()
            .end_position,
        8
    );
    assert_eq!(library.publication(&id).unwrap().position_count, Some(8));

    // An empty report clears the ranges but keeps the last known total.
    library.report_position_ranges(&id, &[]).unwrap();
    assert!(library.chapter_position_ranges(&id).unwrap().is_empty());
    assert_eq!(library.publication(&id).unwrap().position_count, Some(8));
}
