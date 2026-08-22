use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use inkuna_content::test_support::EpubBuilder;
use tempfile::TempDir;

use crate::fonts::FontRegistry;
use crate::session::{CharRange, EngineSession, LayoutEvents, Viewport};
use crate::settings::LayoutSettings;
use crate::test_support::build_epub;

use super::extract_corpus;

const BIG_BUDGET: usize = 32 * 1024 * 1024;

fn registry() -> Arc<FontRegistry> {
    static REG: OnceLock<Arc<FontRegistry>> = OnceLock::new();
    Arc::clone(REG.get_or_init(|| {
        let dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../assets/fonts"));
        FontRegistry::load(dir).expect("repo font set must load")
    }))
}

const CJK_A: &str = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>a</title></head>
<body><h1>第一章</h1><p>月光洒在窗台上，屋里一片寂静。</p></body></html>"#;
const CJK_B: &str = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>b</title></head>
<body><p><ruby>東京<rt>とうきょう</rt></ruby>の空は高かった。</p></body></html>"#;

fn two_chapter_book(dir: &TempDir) -> PathBuf {
    build_epub(
        dir,
        EpubBuilder::new()
            .resource("ch01.xhtml", "application/xhtml+xml", CJK_A.as_bytes())
            .resource("ch02.xhtml", "application/xhtml+xml", CJK_B.as_bytes())
            .spine(&["ch01.xhtml", "ch02.xhtml"]),
    )
}

fn spine(hrefs: &[&str]) -> Vec<String> {
    hrefs.iter().map(|h| h.to_string()).collect()
}

struct NoEvents;
impl LayoutEvents for NoEvents {
    fn first_page_ready(&self, _: u64, _: u32) {}
    fn chapter_ready(&self, _: u64, _: u32, _: u32) {}
}

/// The by-construction identity, asserted: the corpus text IS the
/// projection the session lays out.
#[test]
fn corpus_equals_session_projection() {
    let dir = TempDir::new().expect("tempdir");
    let path = two_chapter_book(&dir);
    let corpus = extract_corpus(&path, &spine(&["OEBPS/ch01.xhtml", "OEBPS/ch02.xhtml"]), BIG_BUDGET);
    assert_eq!(corpus.len(), 2);

    let session = EngineSession::open(
        &path,
        registry(),
        Viewport {
            width: 390.0,
            height: 664.0,
        },
        LayoutSettings::default(),
        None,
        0,
        Arc::new(NoEvents),
    )
    .expect("session opens");
    for idx in 0..2u32 {
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        while !session.is_ready(idx) {
            assert!(std::time::Instant::now() < deadline, "chapter {idx} timed out");
            // Poke the scheduler (queries schedule on miss).
            let _ = session.chapter(idx);
            std::thread::sleep(Duration::from_millis(5));
        }
        let geometry = session.chapter(idx).expect("geometry");
        let laid = session
            .text_range(idx, CharRange { start: 0, end: geometry.char_range.end })
            .expect("laid chapter text");
        assert_eq!(
            corpus[idx as usize].as_deref(),
            Some(laid.as_str()),
            "corpus and layout must index one identical stream (chapter {idx})"
        );
    }
}

#[test]
fn budget_stops_in_spine_order() {
    let dir = TempDir::new().expect("tempdir");
    let path = two_chapter_book(&dir);
    let hrefs = spine(&["OEBPS/ch01.xhtml", "OEBPS/ch02.xhtml"]);
    let full = extract_corpus(&path, &hrefs, BIG_BUDGET);
    let first_len = full[0].as_ref().expect("first chapter extracts").len();
    // A budget that fits chapter 1 but not chapter 2: the overflow and
    // everything after it are None.
    let capped = extract_corpus(&path, &hrefs, first_len);
    assert_eq!(capped[0], full[0]);
    assert_eq!(capped[1], None);
    // A budget that fits nothing: everything is None.
    let none = extract_corpus(&path, &hrefs, 1);
    assert_eq!(none, vec![None, None]);
}

#[test]
fn malformed_resource_is_none_others_fine() {
    let dir = TempDir::new().expect("tempdir");
    let path = build_epub(
        &dir,
        EpubBuilder::new()
            .resource("ch01.xhtml", "application/xhtml+xml", CJK_A.as_bytes())
            .resource("bad.xhtml", "application/xhtml+xml", b"no root element at all")
            .resource("ch03.xhtml", "application/xhtml+xml", CJK_B.as_bytes())
            .spine(&["ch01.xhtml", "bad.xhtml", "ch03.xhtml"]),
    );
    let corpus = extract_corpus(
        &path,
        &spine(&["OEBPS/ch01.xhtml", "OEBPS/bad.xhtml", "OEBPS/ch03.xhtml"]),
        BIG_BUDGET,
    );
    assert!(corpus[0].is_some());
    assert_eq!(corpus[1], None);
    assert!(corpus[2].is_some());
    // A missing resource degrades the same way.
    let missing = extract_corpus(&path, &spine(&["OEBPS/nope.xhtml"]), BIG_BUDGET);
    assert_eq!(missing, vec![None]);
}

#[test]
fn duplicate_spine_entries_alias() {
    let dir = TempDir::new().expect("tempdir");
    let path = two_chapter_book(&dir);
    let corpus = extract_corpus(
        &path,
        &spine(&["OEBPS/ch01.xhtml", "OEBPS/ch01.xhtml", "OEBPS/ch02.xhtml"]),
        BIG_BUDGET,
    );
    assert!(corpus[0].is_some());
    assert_eq!(corpus[0], corpus[1]);
    assert!(corpus[2].is_some());
    assert_ne!(corpus[0], corpus[2]);
}
