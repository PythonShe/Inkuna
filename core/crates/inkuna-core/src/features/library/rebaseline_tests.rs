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

#[test]
fn crash_resume() {
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
    let (first, second) = (ids[0].clone(), ids[1].clone());

    // The hook errors inside book two's transaction: book one commits and
    // is stamped, book two rolls back whole.
    let failing = second.clone();
    super::run_with_hook(
        &library.data_dir,
        &library.data_dir.join("inkuna.db"),
        &library.search.write_handle(),
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
