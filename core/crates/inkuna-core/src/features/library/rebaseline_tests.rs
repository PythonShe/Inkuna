use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use inkuna_content::MAX_TOTAL_TEXT_BYTES;
use inkuna_engine::extract_corpus;

use crate::test_support::write_epub;
use crate::{CoreError, ImportOutcome, Library};

/// Imports the CJK fixture and rewinds it to the pre-engine state the
/// V8 migration leaves behind: `reconciled_at` NULL and a legacy
/// Readium locator in place. Returns `(dir, library, id)`.
fn unreconciled_book(locator: Option<&str>) -> (tempfile::TempDir, Library, String) {
    let dir = tempfile::tempdir().unwrap();
    let epub = dir.path().join("book.epub");
    write_epub(&epub, "月光書房", "紫式部", "ja");
    let library = Library::open(dir.path().join("library")).unwrap();
    // Join the open-spawned pass before mutating rows, so nothing races
    // the manual runs below.
    library.search.wait_for_reconcile();
    let id = match library.import(epub.to_str().unwrap()).unwrap() {
        ImportOutcome::Imported(p) => p.id,
        other => panic!("unexpected {other:?}"),
    };
    {
        let conn = library.writer.lock().unwrap();
        conn.execute(
            "UPDATE publications SET reconciled_at = NULL, locator = ?1 WHERE id = ?2",
            rusqlite::params![locator, id],
        )
        .unwrap();
    }
    (dir, library, id)
}

fn run(library: &Library) {
    super::run(
        &library.data_dir,
        &library.data_dir.join("inkuna.db"),
        &library.search.write_handle(),
        &AtomicBool::new(false),
    );
}

fn publication_row(
    library: &Library,
    id: &str,
) -> (Option<i64>, Option<i64>, Option<String>, Option<i64>) {
    library
        .readers
        .with(|conn| {
            conn.query_row(
                "SELECT position_spine_idx, position_char_offset, locator, reconciled_at
                 FROM publications WHERE id = ?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .map_err(Into::into)
        })
        .unwrap()
}

fn projection_len(library: &Library, id: &str, spine_idx: usize) -> u64 {
    let publication = library.publication(id).unwrap();
    let file = library.data_dir().join(&publication.file_path);
    let hrefs: Vec<String> = library
        .readers
        .with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT href FROM resources WHERE publication_id = ?1 ORDER BY spine_idx",
            )?;
            let rows = stmt.query_map([id], |row| row.get(0))?;
            rows.collect::<Result<_, _>>().map_err(Into::into)
        })
        .unwrap();
    extract_corpus(&file, &hrefs, MAX_TOTAL_TEXT_BYTES)[spine_idx]
        .as_deref()
        .map_or(0, |body| body.chars().count() as u64)
}

#[test]
fn valid_locator_converts() {
    let (_dir, library, id) = unreconciled_book(Some(
        r#"{"href":"OEBPS/text/ch02.xhtml","locations":{"progression":0.5}}"#,
    ));
    run(&library);

    let (spine_idx, char_offset, locator, reconciled_at) = publication_row(&library, &id);
    assert_eq!(spine_idx, Some(1));
    let len = projection_len(&library, &id, 1);
    assert!(len > 0);
    let expected = ((0.5 * len as f64) as u64).min(len - 1);
    assert_eq!(char_offset, Some(expected as i64));
    assert_eq!(locator, None);
    assert!(reconciled_at.is_some());
}

#[test]
fn corrupt_json_defaults_zero() {
    let (_dir, library, id) = unreconciled_book(Some("not json at all"));
    run(&library);

    let (spine_idx, char_offset, locator, reconciled_at) = publication_row(&library, &id);
    assert_eq!(spine_idx, Some(0));
    assert_eq!(char_offset, Some(0));
    assert_eq!(locator, None);
    assert!(reconciled_at.is_some());
}

#[test]
fn unresolvable_href_defaults_zero() {
    let (_dir, library, id) = unreconciled_book(Some(
        r#"{"href":"OEBPS/text/gone.xhtml","locations":{"progression":0.5}}"#,
    ));
    run(&library);

    let (spine_idx, char_offset, locator, _) = publication_row(&library, &id);
    assert_eq!(spine_idx, Some(0));
    assert_eq!(char_offset, Some(0));
    assert_eq!(locator, None);
}

#[test]
fn href_without_progression_gets_chapter_start() {
    let (_dir, library, id) =
        unreconciled_book(Some(r#"{"href":"OEBPS/text/ch02.xhtml","locations":{}}"#));
    run(&library);

    let (spine_idx, char_offset, locator, _) = publication_row(&library, &id);
    assert_eq!(spine_idx, Some(1));
    assert_eq!(char_offset, Some(0));
    assert_eq!(locator, None);
}

#[test]
fn bookmarks_converted_with_defaults() {
    let (_dir, library, id) = unreconciled_book(None);
    // Legacy-shaped rows, as the V8 migration leaves them: locator JSON,
    // coordinate columns NULL.
    {
        let conn = library.writer.lock().unwrap();
        conn.execute(
            "INSERT INTO bookmarks (id, publication_id, locator, progression, created_at)
             VALUES ('bm-valid', ?1,
                     '{\"href\":\"OEBPS/text/ch02.xhtml\",\"locations\":{\"progression\":1.0}}',
                     0.9, 100)",
            [&id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO bookmarks (id, publication_id, locator, progression, created_at)
             VALUES ('bm-broken', ?1, '{broken', 0.1, 200)",
            [&id],
        )
        .unwrap();
    }
    run(&library);

    let rows: Vec<(String, i64, i64)> = library
        .readers
        .with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT locator, position_spine_idx, position_char_offset
                 FROM bookmarks WHERE publication_id = ?1 ORDER BY progression",
            )?;
            let rows = stmt.query_map([&id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
            rows.collect::<Result<_, _>>().map_err(Into::into)
        })
        .unwrap();
    let len = projection_len(&library, &id, 1);
    assert_eq!(rows.len(), 2);
    // Broken locator → (0, 0); valid one clamps progression 1.0 to the
    // last char. Both consumed to ''.
    assert_eq!(rows[0], (String::new(), 0, 0));
    assert_eq!(rows[1], (String::new(), 1, (len - 1) as i64));
}

#[test]
fn idempotent_second_run_noop() {
    let (_dir, library, id) = unreconciled_book(Some(
        r#"{"href":"OEBPS/text/ch02.xhtml","locations":{"progression":0.5}}"#,
    ));
    run(&library);
    let first = publication_row(&library, &id);
    run(&library);
    assert_eq!(publication_row(&library, &id), first);
}

/// Two unreconciled books, `first` most recently opened so the pass
/// visits it first. Returns `(dir, library, first, second)`.
fn two_unreconciled_books() -> (tempfile::TempDir, Library, String, String) {
    let dir = tempfile::tempdir().unwrap();
    let library = Library::open(dir.path().join("library")).unwrap();
    library.search.wait_for_reconcile();

    let mut ids = Vec::new();
    for (name, last_opened) in [("one", 200i64), ("two", 100i64)] {
        let epub = dir.path().join(format!("{name}.epub"));
        write_epub(&epub, &format!("書{name}"), "作者", "ja");
        let id = match library.import(epub.to_str().unwrap()).unwrap() {
            ImportOutcome::Imported(p) => p.id,
            other => panic!("unexpected {other:?}"),
        };
        let conn = library.writer.lock().unwrap();
        conn.execute(
            "UPDATE publications SET reconciled_at = NULL, last_opened_at = ?1 WHERE id = ?2",
            rusqlite::params![last_opened, id],
        )
        .unwrap();
        ids.push(id);
    }
    let second = ids.pop().unwrap();
    let first = ids.pop().unwrap();
    (dir, library, first, second)
}

#[test]
fn crash_resume() {
    let (_dir, library, first, second) = two_unreconciled_books();

    // The hook errors inside book two's transaction: book one commits and
    // is stamped, book two rolls back whole.
    let failing = second.clone();
    super::run_with_hook(
        &library.data_dir,
        &library.data_dir.join("inkuna.db"),
        &library.search.write_handle(),
        &AtomicBool::new(false),
        move |id| {
            if id == failing {
                Err(CoreError::NotFound("injected".into()))
            } else {
                Ok(())
            }
        },
    )
    .unwrap();
    assert!(publication_row(&library, &first).3.is_some());
    assert!(publication_row(&library, &second).3.is_none());

    // The re-run completes the remaining book.
    run(&library);
    assert!(publication_row(&library, &second).3.is_some());
}

#[test]
fn corpus_reindexed_for_search() {
    let (_dir, library, id) = unreconciled_book(None);
    // Corrupt the stored corpus so only the rebaseline can restore it.
    {
        let conn = library.writer.lock().unwrap();
        conn.execute(
            "UPDATE resource_text SET body = 'junk' WHERE resource_id IN
                 (SELECT id FROM resources WHERE publication_id = ?1)",
            [&id],
        )
        .unwrap();
    }
    run(&library);

    // The in-book scan hits the projection at the projection offset…
    let results = library.search_in_book(&id, "月の光", 10).unwrap();
    assert!(results.total >= 1);
    let hit = &results.hits[0];
    assert_eq!(hit.spine_idx, 0);
    let publication = library.publication(&id).unwrap();
    let file = library.data_dir().join(&publication.file_path);
    let hrefs: Vec<String> = library
        .readers
        .with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT href FROM resources WHERE publication_id = ?1 ORDER BY spine_idx",
            )?;
            let rows = stmt.query_map([&id], |row| row.get(0))?;
            rows.collect::<Result<_, _>>().map_err(Into::into)
        })
        .unwrap();
    let projection = extract_corpus(&file, &hrefs, MAX_TOTAL_TEXT_BYTES)[0]
        .clone()
        .unwrap();
    let byte_at = projection.find("月の光").unwrap();
    let expected = projection[..byte_at].chars().count() as u32;
    assert_eq!(hit.char_offset, expected);

    // …and the tantivy index was rebuilt for the book too.
    let all = library.search_all_books("月の光", 10).unwrap();
    assert!(all.iter().any(|hit| hit.publication.id == id));
}

#[test]
fn missing_file_still_stamps_with_defaults() {
    let (_dir, library, id) = unreconciled_book(Some(
        r#"{"href":"OEBPS/text/ch02.xhtml","locations":{"progression":0.5}}"#,
    ));
    let publication = library.publication(&id).unwrap();
    std::fs::remove_file(library.data_dir().join(&publication.file_path)).unwrap();
    let texts_before = library
        .readers
        .with(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM resource_text WHERE resource_id IN
                     (SELECT id FROM resources WHERE publication_id = ?1)",
                [&id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(Into::into)
        })
        .unwrap();
    run(&library);

    // Char lengths are unknowable: the resolvable href falls back to its
    // chapter start; the corpus it had is kept; the book is stamped.
    let (spine_idx, char_offset, locator, reconciled_at) = publication_row(&library, &id);
    assert_eq!(spine_idx, Some(1));
    assert_eq!(char_offset, Some(0));
    assert_eq!(locator, None);
    assert!(reconciled_at.is_some());
    let texts_after = library
        .readers
        .with(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM resource_text WHERE resource_id IN
                     (SELECT id FROM resources WHERE publication_id = ?1)",
                [&id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(Into::into)
        })
        .unwrap();
    assert_eq!(texts_after, texts_before);
}

#[test]
fn fresh_coordinate_survives_stale_locator() {
    let (_dir, library, id) = unreconciled_book(Some(
        r#"{"href":"OEBPS/text/ch02.xhtml","locations":{"progression":0.5}}"#,
    ));
    // A read while the pass was pending: `update_progress` already wrote
    // a fresh coordinate the stale locator's conversion must not clobber.
    {
        let conn = library.writer.lock().unwrap();
        conn.execute(
            "UPDATE publications SET position_spine_idx = 0, position_char_offset = 7
             WHERE id = ?1",
            [&id],
        )
        .unwrap();
    }
    run(&library);

    let (spine_idx, char_offset, locator, reconciled_at) = publication_row(&library, &id);
    assert_eq!((spine_idx, char_offset), (Some(0), Some(7)));
    assert_eq!(locator, None);
    assert!(reconciled_at.is_some());
}

#[test]
fn post_migration_bookmarks_survive_unchanged() {
    let (_dir, library, id) = unreconciled_book(None);
    {
        let conn = library.writer.lock().unwrap();
        // Post-migration shape: empty locator, real coordinate columns.
        conn.execute(
            "INSERT INTO bookmarks
                 (id, publication_id, locator, position_spine_idx, position_char_offset,
                  progression, created_at)
             VALUES ('bm-new', ?1, '', 1, 42, 0.5, 100)",
            [&id],
        )
        .unwrap();
        // Degenerate shape: empty locator, no coordinate — nothing to
        // convert, never a (0, 0) parse-failure default.
        conn.execute(
            "INSERT INTO bookmarks (id, publication_id, locator, progression, created_at)
             VALUES ('bm-empty', ?1, '', 0.1, 200)",
            [&id],
        )
        .unwrap();
    }
    run(&library);

    let rows: Vec<(String, String, Option<i64>, Option<i64>)> = library
        .readers
        .with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, locator, position_spine_idx, position_char_offset
                 FROM bookmarks WHERE publication_id = ?1 ORDER BY id",
            )?;
            let rows = stmt.query_map([&id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })?;
            rows.collect::<Result<_, _>>().map_err(Into::into)
        })
        .unwrap();
    assert_eq!(
        rows,
        vec![
            ("bm-empty".into(), String::new(), None, None),
            ("bm-new".into(), String::new(), Some(1), Some(42)),
        ]
    );
    assert!(publication_row(&library, &id).3.is_some());
}

#[test]
fn position_count_recomputed_even_to_zero() {
    let (_dir, library, id) = unreconciled_book(None);
    {
        let conn = library.writer.lock().unwrap();
        conn.execute("DELETE FROM resources WHERE publication_id = ?1", [&id])
            .unwrap();
        conn.execute(
            "UPDATE publications SET position_count = 999 WHERE id = ?1",
            [&id],
        )
        .unwrap();
    }
    run(&library);

    // No `resources` rows → the recomputed total is 0 and must replace
    // the stale shell-reported count; read-time fallbacks own 0.
    let count: Option<i64> = library
        .readers
        .with(|conn| {
            conn.query_row(
                "SELECT position_count FROM publications WHERE id = ?1",
                [&id],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .unwrap();
    assert_eq!(count, Some(0));
    assert!(publication_row(&library, &id).3.is_some());
}

#[test]
fn cancelled_before_start_stamps_nothing() {
    let (_dir, library, first, second) = two_unreconciled_books();
    super::run_with_hook(
        &library.data_dir,
        &library.data_dir.join("inkuna.db"),
        &library.search.write_handle(),
        &AtomicBool::new(true),
        |_| Ok(()),
    )
    .unwrap();
    assert!(publication_row(&library, &first).3.is_none());
    assert!(publication_row(&library, &second).3.is_none());
}

#[test]
fn cancelled_mid_pass_bails_between_books() {
    let (_dir, library, first, second) = two_unreconciled_books();
    // The hook cancels from inside book one's transaction: book one
    // still commits, the between-books check stops book two.
    let cancel = Arc::new(AtomicBool::new(false));
    let from_hook = Arc::clone(&cancel);
    super::run_with_hook(
        &library.data_dir,
        &library.data_dir.join("inkuna.db"),
        &library.search.write_handle(),
        &cancel,
        move |_| {
            from_hook.store(true, Ordering::Relaxed);
            Ok(())
        },
    )
    .unwrap();
    assert!(publication_row(&library, &first).3.is_some());
    assert!(publication_row(&library, &second).3.is_none());

    // The next run (flag clear, as at a fresh open) finishes book two.
    run(&library);
    assert!(publication_row(&library, &second).3.is_some());
}

#[test]
fn drop_mid_pass_terminates_and_resumes() {
    let (dir, library, id) = unreconciled_book(None);
    drop(library);
    // Reopen spawns the pass and drop joins it immediately: the cancel
    // flag makes the join prompt whether or not the book was reached.
    let reopened = Library::open(dir.path().join("library")).unwrap();
    drop(reopened);
    // A further open completes whatever the cancelled pass left behind.
    let library = Library::open(dir.path().join("library")).unwrap();
    library.search.wait_for_reconcile();
    assert!(publication_row(&library, &id).3.is_some());
}
