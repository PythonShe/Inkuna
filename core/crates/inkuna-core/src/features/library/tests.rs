use super::*;
use crate::test_support::{imported, write_epub, write_epub_with, CoverKind, TocKind};
use crate::{CoreError, Format};

#[test]
fn imports_cjk_epub_and_roundtrips() {
    let dir = tempfile::tempdir().unwrap();
    let epub = dir.path().join("moonlight.epub");
    write_epub(&epub, "月光書房", "紫式部", "ja");

    let data_dir = dir.path().join("library");
    let library = Library::open(&data_dir).unwrap();
    let publication = imported(library.import(epub.to_str().unwrap()).unwrap());
    assert_eq!(publication.title, "月光書房");
    assert_eq!(publication.authors, vec!["紫式部"]);
    assert_eq!(publication.language.as_deref(), Some("ja"));
    assert_eq!(publication.format, Format::Epub);

    // The file was copied into core-owned storage under a relative path.
    assert_eq!(publication.file_path, format!("books/{}.epub", publication.id));
    assert!(data_dir.join(&publication.file_path).is_file());

    let listed = library.list(Shelf::All, Sort::RecentlyAdded).unwrap();
    assert_eq!(listed, vec![publication.clone()]);
    assert_eq!(library.publication(&publication.id).unwrap(), publication);

    library
        .update_progress(&publication.id, "{}", 0.42, None)
        .unwrap();
    assert_eq!(library.list(Shelf::All, Sort::RecentlyAdded).unwrap()[0].progression, 0.42);

    let cover_path = publication.cover_path.clone().unwrap();
    assert!(data_dir.join(&cover_path).is_file());

    library.remove(&publication.id).unwrap();
    assert!(library.list(Shelf::All, Sort::RecentlyAdded).unwrap().is_empty());
    assert!(!data_dir.join(&publication.file_path).exists());
    assert!(!data_dir.join(&cover_path).exists());
    assert!(matches!(
        library.remove(&publication.id),
        Err(CoreError::NotFound(_))
    ));
}


#[test]
fn shelves_and_sorts_partition_the_library() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first.epub");
    write_epub(&first, "最初の本", "著者A", "ja");
    let second = dir.path().join("second.epub");
    write_epub_with(&second, "Second Book", "Author B", "en", TocKind::None, CoverKind::None);

    let library = Library::open(dir.path().join("library")).unwrap();
    let first = imported(library.import(first.to_str().unwrap()).unwrap());
    let second = imported(library.import(second.to_str().unwrap()).unwrap());

    // Nothing opened yet: Reading and Finished are empty, All has both.
    assert!(library.list(Shelf::Reading, Sort::RecentlyAdded).unwrap().is_empty());
    assert!(library.list(Shelf::Finished, Sort::RecentlyAdded).unwrap().is_empty());
    assert_eq!(library.list(Shelf::All, Sort::RecentlyAdded).unwrap().len(), 2);

    // Opening puts a book on the Reading shelf and makes it the
    // recently-opened hero.
    library.session_start(&first.id).unwrap();
    let reading = library.list(Shelf::Reading, Sort::RecentlyOpened).unwrap();
    assert_eq!(reading.len(), 1);
    assert_eq!(reading[0].id, first.id);
    let by_opened = library.list(Shelf::All, Sort::RecentlyOpened).unwrap();
    assert_eq!(by_opened[0].id, first.id);
    assert_eq!(by_opened[1].id, second.id);

    // Finishing moves it off Reading onto Finished.
    library.set_finished(&first.id, true).unwrap();
    assert!(library.list(Shelf::Reading, Sort::RecentlyAdded).unwrap().is_empty());
    let finished = library.list(Shelf::Finished, Sort::RecentlyAdded).unwrap();
    assert_eq!(finished.len(), 1);
    assert_eq!(finished[0].id, first.id);
}

#[test]
fn search_library_matches_cjk_and_casefolds() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.epub");
    write_epub(&a, "月光書房", "紫式部", "ja");
    let b = dir.path().join("b.epub");
    write_epub_with(&b, "Die Straße", "Hans Müller", "de", TocKind::None, CoverKind::None);

    let library = Library::open(dir.path().join("library")).unwrap();
    library.import(a.to_str().unwrap()).unwrap();
    library.import(b.to_str().unwrap()).unwrap();

    // CJK substring over title and author.
    assert_eq!(library.search_library("月光").unwrap().len(), 1);
    assert_eq!(library.search_library("紫").unwrap().len(), 1);
    // Full Unicode case folding, not ASCII LOWER: ß ≡ ss.
    assert_eq!(library.search_library("STRASSE").unwrap().len(), 1);
    assert_eq!(library.search_library("müller").unwrap().len(), 1);
    // No match, and blank queries return nothing.
    assert!(library.search_library("存在しない").unwrap().is_empty());
    assert!(library.search_library("   ").unwrap().is_empty());
}

#[test]
fn bookmarks_roundtrip_sorted_by_progression() {
    let dir = tempfile::tempdir().unwrap();
    let epub = dir.path().join("book.epub");
    write_epub(&epub, "月光書房", "紫式部", "ja");

    let library = Library::open(dir.path().join("library")).unwrap();
    let publication = imported(library.import(epub.to_str().unwrap()).unwrap());

    let late = library
        .add_bookmark(&publication.id, r#"{"locations":{"totalProgression":0.8}}"#, 0.8)
        .unwrap();
    let early = library
        .add_bookmark(&publication.id, r#"{"locations":{"totalProgression":0.2}}"#, 0.2)
        .unwrap();

    let listed = library.bookmarks(&publication.id).unwrap();
    assert_eq!(listed, vec![early.clone(), late.clone()]);

    library.remove_bookmark(&late.id).unwrap();
    assert_eq!(library.bookmarks(&publication.id).unwrap(), vec![early]);
    assert!(matches!(
        library.remove_bookmark(&late.id),
        Err(CoreError::NotFound(_))
    ));
    assert!(matches!(
        library.add_bookmark("missing", "{}", 0.5),
        Err(CoreError::NotFound(_))
    ));
}


#[test]
fn open_sweeps_unreferenced_and_tmp_files() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("library");

    let epub = dir.path().join("book.epub");
    write_epub(&epub, "Kept", "A", "en");
    let kept = {
        let library = Library::open(&data_dir).unwrap();
        imported(library.import(epub.to_str().unwrap()).unwrap())
    };

    // Simulate a crash between file rename and DB commit, plus staging
    // leftovers and an orphaned cover.
    std::fs::write(data_dir.join("books/orphan.epub"), b"orphan").unwrap();
    std::fs::write(data_dir.join("books/half.epub.tmp"), b"partial").unwrap();
    std::fs::write(data_dir.join("covers/orphan.jpg"), b"img").unwrap();

    let library = Library::open(&data_dir).unwrap();
    assert!(!data_dir.join("books/orphan.epub").exists());
    assert!(!data_dir.join("books/half.epub.tmp").exists());
    assert!(!data_dir.join("covers/orphan.jpg").exists());
    // The referenced book survived.
    assert!(data_dir.join(&kept.file_path).is_file());
    assert_eq!(library.list(Shelf::All, Sort::RecentlyAdded).unwrap().len(), 1);
}

#[test]
fn reads_do_not_queue_behind_the_writer() {
    let dir = tempfile::tempdir().unwrap();
    let epub = dir.path().join("book.epub");
    write_epub(&epub, "T", "A", "en");

    let library = Library::open(dir.path().join("library")).unwrap();
    library.import(epub.to_str().unwrap()).unwrap();

    // Hold the writer lock; a read must still complete — if list()
    // touched the writer connection this would deadlock the test.
    let _writer_held = library.writer.lock().unwrap();
    assert_eq!(library.list(Shelf::All, Sort::RecentlyAdded).unwrap().len(), 1);
}
