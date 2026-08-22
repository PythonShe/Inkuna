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
    write_epub_with(
        &en,
        "Another Book",
        "Someone",
        "en",
        TocKind::Ncx,
        CoverKind::None,
    );
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

/// Docs still in the index for `publication_id`, counted through a live
/// query so deleted-but-unmerged docs do not count.
fn indexed_docs(library: &Library, publication_id: &str) -> usize {
    use tantivy::collector::Count;
    use tantivy::query::TermQuery;
    use tantivy::schema::IndexRecordOption;

    let fields = library.search.fields();
    let searcher = library.search.searcher().unwrap();
    searcher
        .search(
            &TermQuery::new(
                tantivy::Term::from_field_text(fields.publication_id, publication_id),
                IndexRecordOption::Basic,
            ),
            &Count,
        )
        .unwrap()
}

#[test]
fn reconcile_drops_docs_whose_publication_left_the_database() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("library");
    let epub = dir.path().join("book.epub");
    write_epub(&epub, "月光書房", "紫式部", "ja");
    let id = {
        let library = Library::open(&data_dir).unwrap();
        let id = imported_id(&library, &epub);
        library.search.wait_for_reconcile();
        assert!(indexed_docs(&library, &id) > 0);
        id
    };

    // Drop the row behind the core's back — what a crash mid-remove, or an
    // older build, leaves behind. Only reconcile can heal it.
    let conn = rusqlite::Connection::open(data_dir.join("inkuna.db")).unwrap();
    conn.execute("DELETE FROM publications WHERE id = ?1", [&id])
        .unwrap();
    drop(conn);

    let library = Library::open(&data_dir).unwrap();
    library.search.wait_for_reconcile();
    assert_eq!(indexed_docs(&library, &id), 0);
}

#[test]
fn reconcile_leaves_an_up_to_date_index_alone() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("library");
    let epub = dir.path().join("book.epub");
    write_epub(&epub, "月光書房", "紫式部", "ja");
    let id = {
        let library = Library::open(&data_dir).unwrap();
        let id = imported_id(&library, &epub);
        library.search.wait_for_reconcile();
        id
    };

    // A second open must recognize the book as already indexed; re-adding
    // it would leave the old docs deleted-but-unmerged and re-index the
    // whole library on every launch.
    let library = Library::open(&data_dir).unwrap();
    library.search.wait_for_reconcile();
    let searcher = library.search.searcher().unwrap();
    let deleted: u32 = searcher
        .segment_readers()
        .iter()
        .map(|segment| segment.num_deleted_docs())
        .sum();
    assert_eq!(deleted, 0);
    assert!(indexed_docs(&library, &id) > 0);
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
fn library_cjk_digraphs_index_as_one_token_per_folded_char() {
    use tantivy::tokenizer::{TokenStream, Tokenizer};

    // ヿ (U+30FF) and ゟ (U+309F) are digraphs: NFKC expands each to two
    // chars. The query side splits a folded run into single chars, so the
    // index has to emit them separately or the phrase can never line up.
    let mut tokenizer = super::tokenize::CjkUnigramTokenizer;
    let mut stream = tokenizer.token_stream("ヿゟ");
    let mut tokens = Vec::new();
    while stream.advance() {
        let token = stream.token();
        tokens.push((token.text.clone(), token.position));
    }
    assert_eq!(
        tokens,
        vec![
            ("コ".to_string(), 0),
            ("ト".to_string(), 1),
            ("よ".to_string(), 2),
            ("り".to_string(), 3),
        ]
    );

    let query = super::tokenize::analyze_query("ヿ");
    assert_eq!(query.cjk_runs, vec![vec!['コ', 'ト']]);
}

#[test]
fn fullwidth_latin_folds_to_ascii() {
    let (_dir, library, id) = library_with_book();

    // ＳＥＣＯＮＤ (full-width) folds to "second" via per-char NFKC.
    let results = library.search_in_book(&id, "ＳＥＣＯＮＤ", 50).unwrap();
    assert_eq!(results.total, 1);
    assert_eq!(results.hits[0].snippet_match, "Second");
}

/// Replaces one resource's stored body directly, bypassing import.
fn set_body(library: &Library, id: &str, spine_idx: u32, body: &str) {
    let conn = library.writer.lock().unwrap();
    conn.execute(
        "UPDATE resource_text SET body = ?1 WHERE resource_id IN
             (SELECT id FROM resources WHERE publication_id = ?2 AND spine_idx = ?3)",
        rusqlite::params![body, id, spine_idx],
    )
    .unwrap();
}

#[test]
fn offsets_original_space_under_fold_expansion() {
    let (_dir, library, id) = library_with_book();
    // `ﬁ` (U+FB01) folds to "fi": folded-space offsets after it drift +1
    // per ligature. The hit's offset must index the ORIGINAL body.
    let body = "oﬃce ﬁrst ﬁne 月光書房 end";
    set_body(&library, &id, 0, body);

    let results = library.search_in_book(&id, "月光書房", 10).unwrap();
    assert_eq!(results.total, 1);
    let hit = &results.hits[0];
    assert_eq!(
        body.chars().nth(hit.char_offset as usize).unwrap(),
        '月',
        "char_offset must start the match in the original body"
    );
    let matched: String = body
        .chars()
        .skip(hit.char_offset as usize)
        .take("月光書房".chars().count())
        .collect();
    assert_eq!(matched, "月光書房");
}

#[test]
fn offsets_original_space_nfkc_contraction() {
    let (_dir, library, id) = library_with_book();
    // `㍿` (U+337F) folds to 株式会社 (1 char → 4): folded offsets after
    // it drift +3. The later hit still indexes the original body.
    let body = "㍿の発表。月光書房は静かだった。";
    set_body(&library, &id, 0, body);

    let results = library.search_in_book(&id, "月光書房", 10).unwrap();
    assert_eq!(results.total, 1);
    let hit = &results.hits[0];
    assert_eq!(body.chars().nth(hit.char_offset as usize).unwrap(), '月');
    // The folded form also matches through the contraction itself.
    let company = library.search_in_book(&id, "株式会社", 10).unwrap();
    assert_eq!(company.total, 1);
    assert_eq!(
        body.chars()
            .nth(company.hits[0].char_offset as usize)
            .unwrap(),
        '㍿'
    );
}

#[test]
fn search_offset_equals_projection_offset() {
    use std::sync::mpsc::{channel, Sender};
    use std::sync::{Arc, OnceLock};

    use crate::{
        CharRange, Coordinate, EngineSession, FontRegistry, LayoutEvents, LayoutSettings, Viewport,
    };

    struct Events(Sender<(u64, u32)>);
    impl LayoutEvents for Events {
        fn first_page_ready(&self, _generation: u64, _spine_idx: u32) {}
        fn chapter_ready(&self, generation: u64, spine_idx: u32, _page_count: u32) {
            let _ = self.0.send((generation, spine_idx));
        }
    }

    fn registry() -> Arc<FontRegistry> {
        static REG: OnceLock<Arc<FontRegistry>> = OnceLock::new();
        Arc::clone(REG.get_or_init(|| {
            let dir =
                std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../assets/fonts"));
            FontRegistry::load(dir).expect("repo font set must load")
        }))
    }

    let (_dir, library, id) = library_with_book();
    // A unique CJK term from the fixture's first chapter.
    let results = library.search_in_book(&id, "窓辺", 10).unwrap();
    assert_eq!(results.total, 1);
    let hit = &results.hits[0];

    // The same offsets, fed to the engine on the same file, name the
    // same characters: end-to-end coordinate identity.
    let publication = library.publication(&id).unwrap();
    let epub_path = library.data_dir().join(&publication.file_path);
    let (tx, rx) = channel();
    let session = EngineSession::open(
        &epub_path,
        registry(),
        Viewport {
            width: 300.0,
            height: 400.0,
        },
        LayoutSettings::default(),
        Some("ja".to_string()),
        hit.spine_idx,
        Arc::new(Events(tx)),
    )
    .unwrap();
    // Wait for the hit's chapter to lay out.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !session.is_ready(hit.spine_idx) {
        assert!(
            std::time::Instant::now() < deadline,
            "chapter never laid out"
        );
        let _ = rx.recv_timeout(std::time::Duration::from_millis(100));
    }
    let len = "窓辺".chars().count() as u64;
    let text = session
        .text_range(
            hit.spine_idx,
            CharRange {
                start: u64::from(hit.char_offset),
                end: u64::from(hit.char_offset) + len,
            },
        )
        .unwrap();
    assert_eq!(text, "窓辺");
    // And the coordinate built from the hit locates a page.
    let location = session
        .locate(Coordinate {
            spine_idx: hit.spine_idx,
            char_offset: u64::from(hit.char_offset),
        })
        .unwrap();
    assert_eq!(location.spine_idx, hit.spine_idx);
    session.close();
}
