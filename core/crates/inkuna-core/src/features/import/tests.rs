use crate::test_support::{imported, write_cbz, write_epub, write_epub_with, CoverKind, TocKind};
use crate::{CoreError, ImportOutcome, Library, Shelf, Sort};

fn count(library: &Library, sql: &str, id: &str) -> i64 {
    library
        .readers
        .with(|conn| conn.query_row(sql, [id], |row| row.get(0)).map_err(Into::into))
        .unwrap()
}


#[test]
fn import_extracts_spine_toc_cover_and_corpus() {
    let dir = tempfile::tempdir().unwrap();
    let epub = dir.path().join("book.epub");
    write_epub(&epub, "月光書房", "紫式部", "ja");

    let data_dir = dir.path().join("library");
    let library = Library::open(&data_dir).unwrap();
    let publication = imported(library.import(epub.to_str().unwrap()).unwrap());

    // Cover extracted as-is under covers/.
    assert_eq!(
        publication.cover_path.as_deref(),
        Some(format!("covers/{}.png", publication.id).as_str())
    );

    // Flattened TOC with CJK titles, fragments, and depth.
    let chapters = library.chapters(&publication.id).unwrap();
    let brief: Vec<(&str, &str, u32)> = chapters
        .iter()
        .map(|c| (c.title.as_str(), c.href.as_str(), c.depth))
        .collect();
    assert_eq!(
        brief,
        vec![
            ("第一章", "OEBPS/text/ch01.xhtml", 0),
            ("第一節", "OEBPS/text/ch01.xhtml#s1", 1),
            ("第二章", "OEBPS/text/ch02.xhtml", 0),
        ]
    );
    assert_eq!(chapters.iter().map(|c| c.idx).collect::<Vec<_>>(), vec![0, 1, 2]);

    // The spine landed in reading order with one text row per resource.
    assert_eq!(
        count(&library, "SELECT COUNT(*) FROM resources WHERE publication_id = ?1", &publication.id),
        2
    );
    let first_body: String = library
        .readers
        .with(|conn| {
            conn.query_row(
                "SELECT rt.body FROM resource_text rt
                 JOIN resources r ON r.id = rt.resource_id
                 WHERE r.publication_id = ?1 ORDER BY r.spine_idx LIMIT 1",
                [&publication.id],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .unwrap();
    assert!(first_body.contains("月の光が窓辺に落ちていた。"));

    // remove() cascades the children.
    library.remove(&publication.id).unwrap();
    assert_eq!(
        count(&library, "SELECT COUNT(*) FROM chapters WHERE publication_id = ?1", &publication.id),
        0
    );
    assert_eq!(
        count(&library, "SELECT COUNT(*) FROM resources WHERE publication_id = ?1", &publication.id),
        0
    );
}

#[test]
fn no_toc_epub_still_builds_a_complete_corpus() {
    let dir = tempfile::tempdir().unwrap();
    let epub = dir.path().join("book.epub");
    write_epub_with(&epub, "無目次", "作者", "ja", TocKind::None, CoverKind::None);

    let library = Library::open(dir.path().join("library")).unwrap();
    let publication = imported(library.import(epub.to_str().unwrap()).unwrap());

    assert!(library.chapters(&publication.id).unwrap().is_empty());
    assert!(publication.cover_path.is_none());
    // The corpus keys off the spine, not the TOC.
    let text_rows = count(
        &library,
        "SELECT COUNT(*) FROM resource_text WHERE resource_id IN
             (SELECT id FROM resources WHERE publication_id = ?1)",
        &publication.id,
    );
    assert_eq!(text_rows, 2);
}

/// A cover href the extension heuristic cannot make sense of degrades to
/// no cover, exactly like unreadable cover bytes do — it must never fail
/// the import. `img.old/cover` under an unknown media type derives the
/// "extension" `old/cover`, and writing `covers/<id>.old/cover` fails on
/// the missing intermediate directory.
#[test]
fn malformed_cover_href_still_imports_without_a_cover() {
    let dir = tempfile::tempdir().unwrap();
    let epub = dir.path().join("book.epub");
    write_epub_with(
        &epub,
        "壊れた表紙",
        "作者",
        "ja",
        TocKind::Nav,
        CoverKind::MalformedHref,
    );

    let data_dir = dir.path().join("library");
    let library = Library::open(&data_dir).unwrap();
    let publication = imported(library.import(epub.to_str().unwrap()).unwrap());

    assert!(publication.cover_path.is_none());
    // The rest of the book imported normally.
    assert_eq!(publication.title, "壊れた表紙");
    assert_eq!(library.chapters(&publication.id).unwrap().len(), 3);
    // And nothing was left behind under covers/.
    assert_eq!(std::fs::read_dir(data_dir.join("covers")).unwrap().count(), 0);
}

#[test]
fn ncx_fallback_supplies_the_toc() {
    let dir = tempfile::tempdir().unwrap();
    let epub = dir.path().join("book.epub");
    write_epub_with(&epub, "旧式目次", "作者", "ja", TocKind::Ncx, CoverKind::None);

    let library = Library::open(dir.path().join("library")).unwrap();
    let publication = imported(library.import(epub.to_str().unwrap()).unwrap());

    let chapters = library.chapters(&publication.id).unwrap();
    assert_eq!(chapters.len(), 3);
    assert_eq!(chapters[1].title, "第一節");
    assert_eq!(chapters[1].href, "OEBPS/text/ch01.xhtml#s1");
    assert_eq!(chapters[1].depth, 1);
}

#[test]
fn rejects_non_epub_naming_the_format() {
    let dir = tempfile::tempdir().unwrap();
    let cbz = dir.path().join("鬼滅の刃 第1巻.cbz");
    write_cbz(&cbz);

    let library = Library::open(dir.path().join("library")).unwrap();
    let err = library.import(cbz.to_str().unwrap()).unwrap_err();
    match err {
        CoreError::UnsupportedFormat(Some(format)) => assert_eq!(format, "cbz"),
        other => panic!("expected UnsupportedFormat with name, got {other:?}"),
    }
    assert!(library.list(Shelf::All, Sort::RecentlyAdded).unwrap().is_empty());
}

#[test]
fn import_is_idempotent_by_content_hash() {
    let dir = tempfile::tempdir().unwrap();
    let epub = dir.path().join("book.epub");
    write_epub(&epub, "月光書房", "紫式部", "ja");

    let data_dir = dir.path().join("library");
    let library = Library::open(&data_dir).unwrap();
    let first = imported(library.import(epub.to_str().unwrap()).unwrap());

    // Same file again.
    match library.import(epub.to_str().unwrap()).unwrap() {
        ImportOutcome::Duplicate(p) => assert_eq!(p.id, first.id),
        other => panic!("expected duplicate, got {other:?}"),
    }

    // Byte-identical copy under another name.
    let copy = dir.path().join("renamed-copy.epub");
    std::fs::copy(&epub, &copy).unwrap();
    match library.import(copy.to_str().unwrap()).unwrap() {
        ImportOutcome::Duplicate(p) => assert_eq!(p.id, first.id),
        other => panic!("expected duplicate, got {other:?}"),
    }

    assert_eq!(library.list(Shelf::All, Sort::RecentlyAdded).unwrap().len(), 1);
    // No stray files: exactly one book in storage.
    let books: Vec<_> = std::fs::read_dir(data_dir.join("books"))
        .unwrap()
        .collect();
    assert_eq!(books.len(), 1);
}


#[test]
fn batch_import_reports_per_item_outcomes_in_order() {
    use crate::BatchImportOutcome;

    let dir = tempfile::tempdir().unwrap();
    let good = dir.path().join("good.epub");
    write_epub(&good, "月光書房", "紫式部", "ja");
    let comic = dir.path().join("comic.cbz");
    write_cbz(&comic);
    let copy = dir.path().join("copy.epub");
    std::fs::copy(&good, &copy).unwrap();

    let library = Library::open(dir.path().join("library")).unwrap();
    let outcomes = library.import_batch(&[
        good.to_str().unwrap().to_string(),
        comic.to_str().unwrap().to_string(),
        copy.to_str().unwrap().to_string(),
    ]);

    assert_eq!(outcomes.len(), 3);
    // Identical content in one batch: exactly one Imported, the copy a
    // Duplicate of it (whichever parallel branch wins the race).
    let (imported, duplicate) = match (&outcomes[0], &outcomes[2]) {
        (BatchImportOutcome::Imported(a), BatchImportOutcome::Duplicate(b))
        | (BatchImportOutcome::Duplicate(b), BatchImportOutcome::Imported(a)) => (a, b),
        other => panic!("expected one Imported + one Duplicate, got {other:?}"),
    };
    assert_eq!(imported.id, duplicate.id);
    match &outcomes[1] {
        BatchImportOutcome::Failed { path, error } => {
            assert_eq!(path, comic.to_str().unwrap());
            assert!(matches!(error, CoreError::UnsupportedFormat(Some(f)) if f == "cbz"));
        }
        other => panic!("expected Failed for the comic, got {other:?}"),
    }
    assert_eq!(library.list(Shelf::All, Sort::RecentlyAdded).unwrap().len(), 1);
}

