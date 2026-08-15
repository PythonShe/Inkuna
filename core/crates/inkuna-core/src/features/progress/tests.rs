use crate::features::library::tests::write_epub;
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

#[test]
fn update_progress_stores_locator_progression_and_positions() {
    let (_dir, library, id) = library_with_book();

    let locator = r#"{"href":"OEBPS/text/ch01.xhtml","locations":{"totalProgression":0.42,"position":12}}"#;
    library.update_progress(&id, locator, 0.42, Some(12)).unwrap();
    library.report_position_count(&id, 300).unwrap();

    let publication = library.publication(&id).unwrap();
    assert_eq!(publication.locator.as_deref(), Some(locator));
    assert_eq!(publication.progression, 0.42);
    assert_eq!(publication.position_count, Some(300));
    assert!(publication.last_opened_at.is_some());
    assert!(publication.finished_at.is_none());

    assert!(matches!(
        library.update_progress("missing", "{}", 0.5, None),
        Err(CoreError::NotFound(_))
    ));
}

#[test]
fn auto_finish_fires_only_on_upward_crossing() {
    let (_dir, library, id) = library_with_book();

    library.update_progress(&id, "{}", 0.9, None).unwrap();
    assert!(library.publication(&id).unwrap().finished_at.is_none());

    // Crossing the threshold from below finishes.
    library.update_progress(&id, "{}", 0.997, None).unwrap();
    assert!(library.publication(&id).unwrap().finished_at.is_some());

    // Explicit unfinish sticks while staying at the end of the book.
    library.set_finished(&id, false).unwrap();
    library.update_progress(&id, "{}", 0.998, None).unwrap();
    library.update_progress(&id, "{}", 1.0, None).unwrap();
    assert!(library.publication(&id).unwrap().finished_at.is_none());

    // Leaving the end and re-reaching it crosses upward again.
    library.update_progress(&id, "{}", 0.5, None).unwrap();
    library.update_progress(&id, "{}", 1.0, None).unwrap();
    assert!(library.publication(&id).unwrap().finished_at.is_some());

    // Explicit finish/unfinish round-trips.
    library.set_finished(&id, false).unwrap();
    assert!(library.publication(&id).unwrap().finished_at.is_none());
    library.set_finished(&id, true).unwrap();
    assert!(library.publication(&id).unwrap().finished_at.is_some());
}
