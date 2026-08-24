use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use inkuna_content::test_support::{EpubBuilder, write_epub_parts};
use tempfile::TempDir;

use crate::error::EngineError;
use crate::fonts::FontRegistry;
use crate::settings::LayoutSettings;
use crate::style::WritingMode;
use crate::test_support::{build_epub, LATIN_DOC};
use crate::text::Coordinate;

use super::{CharRange, EngineSession, LayoutEvents, Viewport};

pub(super) fn registry() -> Arc<FontRegistry> {
    static REG: OnceLock<Arc<FontRegistry>> = OnceLock::new();
    Arc::clone(REG.get_or_init(|| {
        let dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../assets/fonts"));
        FontRegistry::load(dir).expect("repo font set must load")
    }))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Event {
    FirstPage(u64, u32),
    ChapterReady(u64, u32, u32),
    ChapterFailed(u64, u32),
}

pub(super) struct TestEvents(pub(super) Sender<Event>);

impl LayoutEvents for TestEvents {
    fn first_page_ready(&self, generation: u64, spine_idx: u32) {
        let _ = self.0.send(Event::FirstPage(generation, spine_idx));
    }
    fn chapter_ready(&self, generation: u64, spine_idx: u32, page_count: u32) {
        let _ = self.0.send(Event::ChapterReady(generation, spine_idx, page_count));
    }
    fn chapter_failed(&self, generation: u64, spine_idx: u32) {
        let _ = self.0.send(Event::ChapterFailed(generation, spine_idx));
    }
}

pub(super) const TIMEOUT: Duration = Duration::from_secs(60);

/// Receives events until `pred` matches one, panicking on timeout.
pub(super) fn wait_for(rx: &Receiver<Event>, pred: impl Fn(&Event) -> bool) -> Event {
    loop {
        let event = rx.recv_timeout(TIMEOUT).expect("layout event within timeout");
        if pred(&event) {
            return event;
        }
    }
}

pub(super) fn wait_chapter_ready(rx: &Receiver<Event>, generation: u64, spine_idx: u32) -> u32 {
    let event = wait_for(rx, |e| {
        matches!(e, Event::ChapterReady(g, s, _) if *g == generation && *s == spine_idx)
    });
    match event {
        Event::ChapterReady(_, _, count) => count,
        _ => 0,
    }
}

pub(super) fn open(
    path: &PathBuf,
    viewport: Viewport,
    opening: u32,
) -> (Arc<EngineSession>, Receiver<Event>) {
    let (tx, rx) = channel();
    let session = EngineSession::open(
        path,
        registry(),
        viewport,
        LayoutSettings::default(),
        None,
        opening,
        None,
        Arc::new(TestEvents(tx)),
    )
    .expect("session opens");
    (session, rx)
}

pub(super) fn viewport() -> Viewport {
    Viewport {
        width: 200.0,
        height: 240.0,
    }
}

pub(super) fn cjk_doc(paras: usize) -> String {
    let mut body = String::new();
    for _ in 0..paras {
        body.push_str("<p>月光洒在窗台上，屋里一片寂静。他放下手中的书，望向远处的群山。</p>\n");
    }
    format!(
        r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>t</title></head><body>{body}</body></html>"#
    )
}

fn vertical_doc(paras: usize) -> String {
    let mut body = String::new();
    for _ in 0..paras {
        body.push_str(
            "<p><ruby>東京<rt>とうきょう</rt></ruby>の<ruby>空<rt>そら</rt></ruby>は高かった。</p>\n",
        );
    }
    format!(
        r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>t</title>
<style>body {{ writing-mode: vertical-rl; }}</style></head><body>{body}</body></html>"#
    )
}

/// A book of `docs` chapters named ch01.xhtml, ch02.xhtml, …
pub(super) fn book(dir: &TempDir, docs: &[&str]) -> PathBuf {
    let mut builder = EpubBuilder::new();
    let hrefs: Vec<String> = (1..=docs.len()).map(|i| format!("ch{i:02}.xhtml")).collect();
    for (href, doc) in hrefs.iter().zip(docs) {
        builder = builder.resource(href, "application/xhtml+xml", doc.as_bytes());
    }
    let refs: Vec<&str> = hrefs.iter().map(String::as_str).collect();
    build_epub(dir, builder.spine(&refs))
}

#[test]
fn first_page_before_chapter_ready() {
    let dir = TempDir::new().expect("tempdir");
    let doc = cjk_doc(40);
    let path = book(&dir, &[&doc]);
    let (session, rx) = open(&path, viewport(), 0);

    wait_for(&rx, |e| matches!(e, Event::FirstPage(0, 0)));
    // Page 0 is queryable the moment the event fires — the chapter may
    // or may not be complete yet (is_ready is allowed either way).
    let page = session.page(0, 0).expect("page 0 available at first_page_ready");
    assert!(!page.glyph_runs.is_empty());
    let _ = session.is_ready(0);

    let count = wait_chapter_ready(&rx, 0, 0);
    assert!(count > 1, "fixture must span multiple pages");
    let geometry = session.chapter(0).expect("geometry after chapter_ready");
    assert_eq!(geometry.page_count, count);
    assert!(session.is_ready(0));
}

#[test]
fn not_ready_then_ready() {
    let dir = TempDir::new().expect("tempdir");
    let doc = cjk_doc(2);
    let docs: Vec<&str> = (0..6).map(|_| doc.as_str()).collect();
    let path = book(&dir, &docs);
    let (session, rx) = open(&path, viewport(), 0);

    // Chapter 3 is outside the opening prefetch radius (±2 of 0), so
    // the first query misses, schedules, and returns NotReady.
    match session.chapter(3) {
        Err(EngineError::NotReady) => {}
        other => panic!("expected NotReady before layout, got {other:?}"),
    }
    wait_chapter_ready(&rx, 0, 3);
    let geometry = session.chapter(3).expect("geometry after ready");
    assert!(geometry.page_count >= 1);
    assert!(geometry.char_range.end > 0);
}

#[test]
fn update_layout_bumps_generation_and_invalidates() {
    let dir = TempDir::new().expect("tempdir");
    let doc = cjk_doc(10);
    let path = book(&dir, &[&doc]);
    let (session, rx) = open(&path, viewport(), 0);
    wait_chapter_ready(&rx, 0, 0);
    let before = session.chapter(0).expect("gen-0 geometry");
    assert_eq!(before.generation, 0);

    session.update_layout(
        Viewport {
            width: 320.0,
            height: 480.0,
        },
        LayoutSettings::default(),
    );
    wait_chapter_ready(&rx, 1, 0);
    let after = session.chapter(0).expect("gen-1 geometry");
    assert_eq!(after.generation, 1);
    // Once a gen-1 event has been observed, gen 0 is dead: no later
    // event may carry it.
    while let Ok(event) = rx.recv_timeout(Duration::from_millis(200)) {
        let generation = match event {
            Event::FirstPage(g, _) => g,
            Event::ChapterReady(g, _, _) => g,
            Event::ChapterFailed(g, _) => g,
        };
        assert_eq!(generation, 1, "stale generation re-observed: {event:?}");
    }
}

#[test]
fn locate_hit_test_round_trip() {
    let dir = TempDir::new().expect("tempdir");
    let horizontal = cjk_doc(20);
    let vertical = vertical_doc(20);
    for doc in [&horizontal, &vertical] {
        let path = book(&dir, &[doc]);
        let (session, rx) = open(&path, viewport(), 0);
        let count = wait_chapter_ready(&rx, 0, 0);
        assert!(count > 1);
        for page_idx in 0..count {
            for x in [10.0, 60.0, 120.0, 190.0] {
                for y in [10.0, 80.0, 160.0, 230.0] {
                    let hit = session
                        .hit_test(0, page_idx, x, y)
                        .expect("hit test resolves");
                    let location = session.locate(hit.coordinate).expect("locate resolves");
                    assert_eq!(
                        location.page_idx, page_idx,
                        "round trip at ({x},{y}) page {page_idx}"
                    );
                }
            }
        }
        session.close();
    }
}

#[test]
fn selection_rects_cover_range() {
    let dir = TempDir::new().expect("tempdir");
    let doc = cjk_doc(3);
    let path = book(&dir, &[&doc]);
    let (session, rx) = open(&path, viewport(), 0);
    wait_chapter_ready(&rx, 0, 0);
    let range = CharRange { start: 2, end: 12 };
    let rects = session.selection_rects(0, range).expect("selection rects");
    assert!(!rects.is_empty());
    for r in &rects {
        assert!(r.rect.width > 0.0 && r.rect.height > 0.0);
        assert_eq!(r.writing_mode, WritingMode::HorizontalTb);
        // The union covers the range's cells: the center of every rect
        // resolves back into the range.
        let hit = session
            .hit_test(
                0,
                0,
                r.rect.x + r.rect.width / 2.0,
                r.rect.y + r.rect.height / 2.0,
            )
            .expect("center resolves");
        assert!(hit.coordinate.char_offset >= range.start);
        assert!(hit.coordinate.char_offset < range.end);
    }

    // Vertical: writing mode carried on each rect.
    let vdoc = vertical_doc(3);
    let vpath = book(&dir, &[&vdoc]);
    let (vsession, vrx) = open(&vpath, viewport(), 0);
    wait_chapter_ready(&vrx, 0, 0);
    let vrects = vsession
        .selection_rects(0, CharRange { start: 0, end: 5 })
        .expect("vertical selection rects");
    assert!(!vrects.is_empty());
    for r in &vrects {
        assert_eq!(r.writing_mode, WritingMode::VerticalRl);
        assert!(r.rect.width > 0.0 && r.rect.height > 0.0);
    }
    // match_rects is exactly selection_rects over (offset, len).
    let matched = vsession.match_rects(0, 0, 5).expect("match rects");
    assert_eq!(matched, vrects);
}

#[test]
fn locate_href_fragment() {
    let dir = TempDir::new().expect("tempdir");
    let path = book(&dir, &[LATIN_DOC]);
    let (session, rx) = open(
        &path,
        Viewport {
            width: 390.0,
            height: 664.0,
        },
        0,
    );
    // Fragment-free: resolved from the spine model alone.
    let top = session
        .locate_href("OEBPS/ch01.xhtml", None)
        .expect("chapter href resolves without layout");
    assert_eq!(top, Coordinate { spine_idx: 0, char_offset: 0 });

    wait_chapter_ready(&rx, 0, 0);
    let anchor = session
        .locate_href("OEBPS/ch01.xhtml", Some("landfall"))
        .expect("fragment resolves after layout");
    assert_eq!(anchor.spine_idx, 0);
    // The anchor pins to the first char of the "Landfall" heading.
    let text = session
        .text_range(
            0,
            CharRange {
                start: anchor.char_offset,
                end: anchor.char_offset + 8,
            },
        )
        .expect("text at anchor");
    assert_eq!(text, "Landfall");

    match session.locate_href("OEBPS/ch01.xhtml", Some("no-such-anchor")) {
        Err(EngineError::AnchorNotFound { .. }) => {}
        other => panic!("expected AnchorNotFound, got {other:?}"),
    }
    match session.locate_href("OEBPS/missing.xhtml", None) {
        Err(EngineError::AnchorNotFound { .. }) => {}
        other => panic!("expected AnchorNotFound for unknown href, got {other:?}"),
    }
}

#[test]
fn word_at_cjk() {
    let dir = TempDir::new().expect("tempdir");
    let doc = cjk_doc(1);
    let path = book(&dir, &[&doc]);
    let (session, rx) = open(&path, viewport(), 0);
    wait_chapter_ready(&rx, 0, 0);
    // Projection text: "月光洒在窗台上，屋里一片寂静。…" — offset 0
    // sits inside the word 月光.
    let word = session
        .word_at(Coordinate {
            spine_idx: 0,
            char_offset: 0,
        })
        .expect("word at CJK offset");
    assert!(word.start == 0 && word.end > word.start);
    let text = session.text_range(0, word).expect("word text");
    assert_eq!(text, "月光", "ICU segments 月光 as one word");

    // Latin sanity: an offset inside a word covers the whole word.
    let lpath = book(&dir, &[LATIN_DOC]);
    let (lsession, lrx) = open(
        &lpath,
        Viewport {
            width: 390.0,
            height: 664.0,
        },
        0,
    );
    wait_chapter_ready(&lrx, 0, 0);
    let full = lsession.chapter(0).expect("geometry").char_range;
    let all = lsession.text_range(0, full).expect("full text");
    let offset = all
        .chars()
        .collect::<Vec<_>>()
        .windows(6)
        .position(|w| w.iter().collect::<String>() == "Voyage")
        .expect("fixture text contains Voyage") as u64;
    let word = lsession
        .word_at(Coordinate {
            spine_idx: 0,
            char_offset: offset + 2,
        })
        .expect("word at latin offset");
    let text = lsession.text_range(0, word).expect("latin word text");
    assert_eq!(text, "Voyage");
}

#[test]
fn fixed_layout_rejected_at_open() {
    let dir = TempDir::new().expect("tempdir");
    let path = build_epub(
        &dir,
        EpubBuilder::new()
            .resource("ch01.xhtml", "application/xhtml+xml", LATIN_DOC.as_bytes())
            .spine(&["ch01.xhtml"])
            .pre_paginated(),
    );
    let (tx, _rx) = channel();
    match EngineSession::open(
        &path,
        registry(),
        viewport(),
        LayoutSettings::default(),
        None,
        0,
        None,
        Arc::new(TestEvents(tx)),
    ) {
        Err(EngineError::UnsupportedContent { detail }) => {
            assert_eq!(detail, "fixed-layout");
        }
        other => panic!("expected UnsupportedContent, got {:?}", other.map(|_| ())),
    }
}

#[test]
fn declared_fixed_layout_is_not_downgraded_by_a_reflowable_itemref() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("declared-fixed.epub");
    write_epub_parts(
        &path,
        r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>固定本</dc:title>
    <meta property="rendition:layout">pre-paginated</meta>
  </metadata>
  <manifest>
    <item id="cover" href="cover.xhtml" media-type="application/xhtml+xml"/>
    <item id="page" href="page.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="cover" properties="rendition:layout-reflowable"/>
    <itemref idref="page"/>
  </spine>
</package>"#,
        &[
            ("cover.xhtml", "<html><body><p>表紙</p></body></html>"),
            ("page.xhtml", "<html><body><p>本文</p></body></html>"),
        ],
    );
    let (tx, _rx) = channel();

    match EngineSession::open(
        &path,
        registry(),
        viewport(),
        LayoutSettings::default(),
        None,
        0,
        None,
        Arc::new(TestEvents(tx)),
    ) {
        Err(EngineError::UnsupportedContent { detail }) => assert_eq!(detail, "fixed-layout"),
        other => panic!("expected UnsupportedContent, got {:?}", other.map(|_| ())),
    }
}

#[test]
fn fixed_spine_item_fails_without_refusing_a_reflowable_publication() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("mixed-layout.epub");
    write_epub_parts(
        &path,
        r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>混在</dc:title></metadata>
  <manifest>
    <item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/>
    <item id="map" href="map.xhtml" media-type="application/xhtml+xml"/>
    <item id="c2" href="c2.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="c1"/>
    <itemref idref="map" properties="rendition:layout-pre-paginated"/>
    <itemref idref="c2"/>
  </spine>
</package>"#,
        &[
            ("c1.xhtml", "<html><body><p>第一章</p></body></html>"),
            ("map.xhtml", "<html><body><p>地図</p></body></html>"),
            ("c2.xhtml", "<html><body><p>第二章</p></body></html>"),
        ],
    );
    let (session, rx) = open(&path, viewport(), 0);

    wait_chapter_ready(&rx, 0, 0);
    let event = rx.recv_timeout(TIMEOUT).expect("fixed item result");
    assert!(matches!(event, Event::ChapterFailed(0, 1)), "{event:?}");
    wait_chapter_ready(&rx, 0, 2);
    assert!(session.chapter(0).is_ok());
    assert!(matches!(
        session.chapter(1),
        Err(EngineError::UnsupportedContent { .. })
    ));
    assert!(session.chapter(2).is_ok());
}

#[test]
fn failed_chapter_scoped() {
    let dir = TempDir::new().expect("tempdir");
    let good = cjk_doc(2);
    let path = book(&dir, &[&good, "no root element here at all", &good]);
    let (session, rx) = open(&path, viewport(), 0);
    // Jobs run in spine order (opening chapter, then its neighbors), so
    // the failure lands between the two successes.
    wait_chapter_ready(&rx, 0, 0);
    // The garbage chapter fails closed and SAYS SO: the event is what
    // takes a shell out of its loading state — no polling anywhere.
    wait_for(&rx, |e| matches!(e, Event::ChapterFailed(0, 1)));
    wait_chapter_ready(&rx, 0, 2);
    assert!(session.chapter(0).is_ok());
    assert!(session.chapter(2).is_ok());
    // ...and the event's promise holds: the query path is terminal, not
    // `NotReady`, from the moment the event arrives.
    match session.chapter(1) {
        Err(EngineError::UnsupportedContent { .. }) => {}
        other => panic!("expected UnsupportedContent for garbage chapter, got {other:?}"),
    }
    match session.page(1, 0) {
        Err(EngineError::UnsupportedContent { .. }) => {}
        other => panic!("expected UnsupportedContent page, got {other:?}"),
    }
    // The book stays usable.
    assert!(session.page(0, 0).is_ok());
}

#[test]
fn close_makes_everything_not_ready() {
    let dir = TempDir::new().expect("tempdir");
    let doc = cjk_doc(2);
    let path = book(&dir, &[&doc]);
    let (session, rx) = open(&path, viewport(), 0);
    wait_chapter_ready(&rx, 0, 0);
    assert!(session.chapter(0).is_ok());
    session.close();
    assert!(!session.is_ready(0));
    for result in [
        session.chapter(0).map(|_| ()),
        session.page(0, 0).map(|_| ()),
        session.locate_href("OEBPS/ch01.xhtml", None).map(|_| ()),
        session.resource("OEBPS/ch01.xhtml").map(|_| ()),
    ] {
        match result {
            Err(EngineError::NotReady) => {}
            other => panic!("expected NotReady on closed session, got {other:?}"),
        }
    }
    // Idempotent.
    session.close();
}

#[test]
fn coordinates_survive_update_layout() {
    let dir = TempDir::new().expect("tempdir");
    let doc = cjk_doc(20);
    let path = book(&dir, &[&doc]);
    let (session, rx) = open(&path, viewport(), 0);
    wait_chapter_ready(&rx, 0, 0);

    let c = Coordinate {
        spine_idx: 0,
        char_offset: 120,
    };
    let char_at = session
        .text_range(0, CharRange { start: 120, end: 121 })
        .expect("char at coordinate");
    let before = session.locate(c).expect("locate before relayout");
    let before_range = session
        .page_char_range(0, before.page_idx)
        .expect("page range before");
    assert!(before_range.start <= 120 && 120 < before_range.end);

    session.update_layout(
        Viewport {
            width: 300.0,
            height: 500.0,
        },
        LayoutSettings::default(),
    );
    wait_chapter_ready(&rx, 1, 0);
    let after = session.locate(c).expect("locate after relayout");
    assert_eq!(after.generation, 1);
    let after_range = session
        .page_char_range(0, after.page_idx)
        .expect("page range after");
    assert!(after_range.start <= 120 && 120 < after_range.end);
    // The canonical stream is appearance-independent: the same char
    // lives at the same offset.
    let char_after = session
        .text_range(0, CharRange { start: 120, end: 121 })
        .expect("char after relayout");
    assert_eq!(char_at, char_after);
}

#[test]
fn word_index_cached_at_publish() {
    use std::sync::atomic::Ordering;

    let dir = TempDir::new().expect("tempdir");
    let path = book(&dir, &[LATIN_DOC]);
    let (session, rx) = open(
        &path,
        Viewport {
            width: 390.0,
            height: 664.0,
        },
        0,
    );
    wait_chapter_ready(&rx, 0, 0);

    // Structure: the worker published the chapter's text index with the
    // chapter itself — `word_at` answers from these cached boundaries
    // and never re-segments per call.
    {
        let generation = session.shared.generation.load(Ordering::Acquire);
        let mut inner = session.shared.lock();
        match inner.cache.get(0, generation) {
            Some(super::cache::SlotState::Ready(data)) => {
                assert!(data.index.chars > 0, "index published with the chapter");
                assert!(data.index.word_bounds.len() >= 2);
                assert_eq!(data.index.word_bounds.first(), Some(&0));
                assert_eq!(data.index.word_bounds.last(), Some(&data.index.chars));
                assert!(!data.index.stride_bytes.is_empty());
            }
            _ => panic!("chapter 0 must be Ready"),
        }
    }

    // Correctness stays identical across repeated calls.
    let c = Coordinate {
        spine_idx: 0,
        char_offset: 4,
    };
    let first = session.word_at(c).expect("word at offset");
    assert!(first.start <= 4 && first.end > first.start);
    for _ in 0..3 {
        assert_eq!(session.word_at(c).expect("repeated word_at"), first);
    }
}

/// Drops the LAST `Arc<EngineSession>` inside a [`LayoutEvents`]
/// callback: `Drop` → `close()` runs ON the worker thread, which must
/// skip the self-join instead of deadlocking (std panics on a thread
/// joining itself).
struct DropInWorker {
    slot: std::sync::Mutex<Option<Arc<EngineSession>>>,
    gate: std::sync::Mutex<Option<Receiver<()>>>,
    done: std::sync::atomic::AtomicBool,
}

impl LayoutEvents for DropInWorker {
    fn first_page_ready(&self, _: u64, _: u32) {}
    fn chapter_failed(&self, _: u64, _: u32) {}
    fn chapter_ready(&self, _: u64, _: u32, _: u32) {
        let gate = self.gate.lock().expect("gate lock").take();
        let Some(gate) = gate else { return };
        // Wait until the test thread has parked its only Arc in `slot`.
        gate.recv_timeout(TIMEOUT).expect("armed within timeout");
        let arc = self.slot.lock().expect("slot lock").take();
        drop(arc); // the last Arc: close() runs on this worker thread
        self.done.store(true, std::sync::atomic::Ordering::Release);
    }
}

#[test]
fn drop_inside_callback_does_not_deadlock() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Instant;

    let dir = TempDir::new().expect("tempdir");
    let doc = cjk_doc(1);
    let path = book(&dir, &[&doc]);
    let (gate_tx, gate_rx) = channel();
    let events = Arc::new(DropInWorker {
        slot: std::sync::Mutex::new(None),
        gate: std::sync::Mutex::new(Some(gate_rx)),
        done: AtomicBool::new(false),
    });
    let session = EngineSession::open(
        &path,
        registry(),
        viewport(),
        LayoutSettings::default(),
        None,
        0,
        None,
        Arc::clone(&events) as Arc<dyn LayoutEvents>,
    )
    .expect("session opens");
    // Park the ONLY Arc inside the events object, then arm the callback.
    *events.slot.lock().expect("slot lock") = Some(session);
    gate_tx.send(()).expect("gate send");

    let deadline = Instant::now() + TIMEOUT;
    while !events.done.load(Ordering::Acquire) {
        assert!(
            Instant::now() < deadline,
            "close() from the worker thread must not deadlock or panic"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
