use crate::test_support::{write_epub, write_epub_with, CoverKind, TocKind};
use crate::{CoreError, ImportOutcome, Library};

fn imported_id(library: &Library, path: &std::path::Path) -> String {
    match library.import(path.to_str().unwrap()).unwrap() {
        ImportOutcome::Imported(p) => p.id,
        other => panic!("unexpected {other:?}"),
    }
}

fn library_with_book() -> (tempfile::TempDir, Library, String) {
    let dir = tempfile::tempdir().unwrap();
    let epub = dir.path().join("book.epub");
    write_epub(&epub, "月光書房", "紫式部", "ja");
    let library = Library::open(dir.path().join("library")).unwrap();
    let id = imported_id(&library, &epub);
    (dir, library, id)
}

// Fixture texts: ch01 = "第一章\n月の光が窓辺に落ちていた。",
// ch02 = "Second chapter text."

#[test]
fn in_book_single_cjk_char_matches() {
    let (_dir, library, id) = library_with_book();

    let results = library.search_in_book(&id, "月", 50).unwrap();
    assert_eq!(results.total, 1);
    let hit = &results.hits[0];
    assert_eq!(hit.spine_idx, 0);
    assert_eq!(hit.href, "OEBPS/text/ch01.xhtml");
    assert_eq!(hit.snippet_match, "月");
    assert!(hit.snippet_post.starts_with("の光"));
    assert!((0.0..=1.0).contains(&hit.progression));
}

#[test]
fn in_book_matches_partial_words_and_folds_case() {
    let (_dir, library, id) = library_with_book();

    // Partial word, wrong case: an exact scan must still find it.
    let results = library.search_in_book(&id, "CHAP", 50).unwrap();
    assert_eq!(results.total, 1);
    assert_eq!(results.hits[0].spine_idx, 1);
    assert_eq!(results.hits[0].snippet_match, "chap");
    assert!(results.hits[0].snippet_pre.ends_with("Second "));
}

#[test]
fn in_book_counts_all_occurrences_and_caps_hits() {
    let (_dir, library, id) = library_with_book();

    // "e" appears in "Second chapter text." exactly three times.
    let all = library.search_in_book(&id, "e", 50).unwrap();
    assert_eq!(all.total, 3);
    assert_eq!(all.hits.len() as u32, all.total);

    let capped = library.search_in_book(&id, "e", 1).unwrap();
    assert_eq!(capped.total, all.total);
    assert_eq!(capped.hits.len(), 1);
}

#[test]
fn in_book_reports_overlapping_occurrences() {
    let folded = super::fold::fold_text("哈哈哈");
    let needle = super::fold::fold_query("哈哈");

    let occurrences: Vec<_> = folded.occurrences(&needle).collect();
    assert_eq!(occurrences, vec![(0, 2), (1, 3)]);
}

#[test]
fn in_book_char_offsets_are_char_based() {
    let (_dir, library, id) = library_with_book();

    let results = library.search_in_book(&id, "光", 50).unwrap();
    let hit = &results.hits[0];
    // "第一章\n月の光…": the 光 is the 7th char (offset 6) — a byte
    // offset would be three times that.
    assert_eq!(hit.char_offset, 6);
}

#[test]
fn in_book_empty_query_and_missing_book() {
    let (_dir, library, id) = library_with_book();

    assert_eq!(library.search_in_book(&id, "  ", 50).unwrap().total, 0);
    assert!(matches!(
        library.search_in_book("missing", "月", 50),
        Err(CoreError::NotFound(_))
    ));
}

#[test]
fn library_wide_ranked_search_across_books() {
    let dir = tempfile::tempdir().unwrap();
    let library = Library::open(dir.path().join("library")).unwrap();
    let jp = dir.path().join("jp.epub");
    write_epub(&jp, "月光書房", "紫式部", "ja");
    let jp_id = imported_id(&library, &jp);
    let en = dir.path().join("en.epub");
    // Same spine texts, different metadata → different content hash.
    write_epub_with(&en, "Another Book", "Someone", "en", TocKind::Ncx, CoverKind::None);
    imported_id(&library, &en);

    // Both books carry the same texts, so both match; single CJK char.
    let hits = library.search_all_books("月", 10).unwrap();
    assert_eq!(hits.len(), 2);

    // CJK substring straddling jieba word boundaries still matches via
    // the unigram phrase (光が窓 crosses 光/が/窓辺 tokens).
    let hits = library.search_all_books("光が窓", 10).unwrap();
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].excerpt_match, "光が窓");

    // Latin word search, case-folded, with a pinned excerpt.
    let hits = library.search_all_books("SECOND chapter", 10).unwrap();
    assert_eq!(hits.len(), 2);
    assert!(hits[0].excerpt_match.to_lowercase().contains("second"));

    // No match at all.
    assert!(library.search_all_books("nowhere", 10).unwrap().is_empty());
    assert!(library.search_all_books("  ", 10).unwrap().is_empty());

    // Removal drops the book from results immediately.
    library.remove(&jp_id).unwrap();
    let hits = library.search_all_books("月", 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_ne!(hits[0].publication.id, jp_id);
}

#[test]
fn reconcile_rebuilds_a_missing_index() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("library");
    let epub = dir.path().join("book.epub");
    write_epub(&epub, "月光書房", "紫式部", "ja");
    {
        let library = Library::open(&data_dir).unwrap();
        imported_id(&library, &epub);
        library.search.wait_for_reconcile();
    }

    // Blow the index away; the corpus in SQLite is the source of truth.
    std::fs::remove_dir_all(data_dir.join("index")).unwrap();
    let library = Library::open(&data_dir).unwrap();
    library.search.wait_for_reconcile();
    let hits = library.search_all_books("窓辺", 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].publication.title, "月光書房");
}

#[test]
fn library_cjk_tokens_use_folded_compatibility_forms_and_new_han_blocks() {
    use tantivy::tokenizer::{TokenStream, Tokenizer};

    let mut tokenizer = super::tokenize::CjkUnigramTokenizer;
    let mut stream = tokenizer.token_stream("ﾊ\u{30000}");
    assert!(stream.advance());
    assert_eq!(stream.token().text, "ハ");
    assert!(stream.advance());
    assert_eq!(stream.token().text, "\u{30000}");
    assert!(!stream.advance());

    let query = super::tokenize::analyze_query("ﾊ");
    assert_eq!(query.cjk_runs, vec![vec!['ハ']]);
}

#[test]
fn fullwidth_latin_folds_to_ascii() {
    let (_dir, library, id) = library_with_book();

    // ＳＥＣＯＮＤ (full-width) folds to "second" via per-char NFKC.
    let results = library.search_in_book(&id, "ＳＥＣＯＮＤ", 50).unwrap();
    assert_eq!(results.total, 1);
    assert_eq!(results.hits[0].snippet_match, "Second");
}
