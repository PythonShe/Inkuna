//! The two total, always-answerable query surfaces the shells set up
//! their pager on before anything is laid out: [`EngineSession::is_rtl`]
//! (publication-level, known at open) and
//! [`EngineSession::published_page_count`] (progressive cache count that
//! never blocks, never schedules, and never fails).

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, Weak};

use inkuna_content::test_support::EpubBuilder;
use tempfile::TempDir;

use crate::error::EngineError;
use crate::settings::LayoutSettings;
use crate::test_support::{LATIN_DOC, build_epub};

use super::model::LayoutEvents;
use super::session::EngineSession;
use super::tests::{
    Event, TIMEOUT, TestEvents, book, cjk_doc, open, registry, viewport, wait_chapter_ready,
    wait_for,
};

/// Builds a book whose spine `page-progression-direction` is `rtl`.
fn rtl_book(dir: &TempDir, doc: &str) -> std::path::PathBuf {
    build_epub(
        dir,
        EpubBuilder::new()
            .language("ja")
            .resource("ch01.xhtml", "application/xhtml+xml", doc.as_bytes())
            .spine(&["ch01.xhtml"])
            .rtl_progression(),
    )
}

/// `is_rtl` is publication-level: it answers from the OPF read at open,
/// so it is already correct on page 0 — the whole reason it exists next
/// to `ChapterGeometry::rtl_progression`, which needs a complete
/// chapter. Both must agree once the chapter does land.
#[test]
fn is_rtl_true_for_rtl_publication_before_any_layout() {
    let dir = TempDir::new().expect("tempdir");
    let doc = cjk_doc(40);
    let path = rtl_book(&dir, &doc);
    let (session, rx) = open(&path, viewport(), 0);

    // Read before touching any query that could lay a chapter out.
    assert!(session.is_rtl(), "RTL progression known at open");

    // And it still agrees with the per-chapter mirror once one lands.
    wait_chapter_ready(&rx, 0, 0);
    let geometry = session.chapter(0).expect("geometry after chapter_ready");
    assert!(geometry.rtl_progression);
    assert_eq!(session.is_rtl(), geometry.rtl_progression);
}

#[test]
fn is_rtl_false_for_ltr_publication() {
    let dir = TempDir::new().expect("tempdir");
    let doc = cjk_doc(40);
    let path = book(&dir, &[&doc]);
    let (session, rx) = open(&path, viewport(), 0);

    assert!(!session.is_rtl(), "no page-progression-direction is LTR");

    wait_chapter_ready(&rx, 0, 0);
    let geometry = session.chapter(0).expect("geometry after chapter_ready");
    assert!(!geometry.rtl_progression);
    assert_eq!(session.is_rtl(), geometry.rtl_progression);
}

/// The airtight version of "needs no laid-out chapter": a book whose
/// only spine resource can never be laid out at all. `chapter()` is
/// terminally unavailable, and `is_rtl` answers anyway — as it does on a
/// closed session, whose worker is gone entirely.
#[test]
fn is_rtl_answers_when_no_chapter_can_ever_lay_out() {
    let dir = TempDir::new().expect("tempdir");
    let path = build_epub(
        &dir,
        EpubBuilder::new()
            .resource(
                "ch01.xhtml",
                "application/xhtml+xml",
                b"no root element here at all",
            )
            .spine(&["ch01.xhtml"])
            .rtl_progression(),
    );
    let (session, rx) = open(&path, viewport(), 0);

    wait_for(&rx, |e| matches!(e, Event::ChapterFailed(0, 0)));
    assert!(
        matches!(
            session.chapter(0),
            Err(EngineError::UnsupportedContent { .. })
        ),
        "the only chapter must be terminally unavailable"
    );
    assert!(session.is_rtl(), "still answers with nothing laid out");

    session.close();
    assert!(!session.is_ready(0));
    assert!(session.is_rtl(), "still answers on a closed session");
}

/// Reads `published_page_count` from INSIDE `first_page_ready`, which
/// the worker fires on its own thread with exactly one page published
/// and the session mutex released. Gated so the probe cannot run before
/// the test parks the session handle; the worker is blocked in the
/// callback throughout, so the observation is exact rather than racy.
struct FirstPageProbe {
    session: Mutex<Option<Weak<EngineSession>>>,
    gate: Mutex<Option<Receiver<()>>>,
    observed: AtomicU32,
    forward: Sender<Event>,
}

impl LayoutEvents for FirstPageProbe {
    fn first_page_ready(&self, generation: u64, spine_idx: u32) {
        if let Some(gate) = self.gate.lock().expect("gate lock").take() {
            let _ = gate.recv_timeout(TIMEOUT);
            let parked = self.session.lock().expect("session lock").clone();
            if let Some(session) = parked.and_then(|w| w.upgrade()) {
                // No deadlock: the worker holds no lock while it calls
                // back, and this query takes only the session mutex.
                self.observed
                    .store(session.published_page_count(spine_idx), Ordering::Release);
            }
        }
        let _ = self.forward.send(Event::FirstPage(generation, spine_idx));
    }
    fn chapter_ready(&self, generation: u64, spine_idx: u32, page_count: u32) {
        let _ = self
            .forward
            .send(Event::ChapterReady(generation, spine_idx, page_count));
    }
    fn chapter_failed(&self, generation: u64, spine_idx: u32) {
        let _ = self
            .forward
            .send(Event::ChapterFailed(generation, spine_idx));
    }
}

/// The count rises with layout: 0 before the chapter starts, exactly 1
/// at `first_page_ready`, and the full `page_count` once the chapter
/// completes — with no error case anywhere along the way.
#[test]
fn published_page_count_rises_from_zero_to_page_count() {
    let dir = TempDir::new().expect("tempdir");
    let doc = cjk_doc(40);
    let path = book(&dir, &[&doc]);

    let (gate_tx, gate_rx) = channel();
    let (event_tx, rx) = channel();
    let probe = Arc::new(FirstPageProbe {
        session: Mutex::new(None),
        gate: Mutex::new(Some(gate_rx)),
        observed: AtomicU32::new(u32::MAX),
        forward: event_tx,
    });
    let session = EngineSession::open(
        &path,
        registry(),
        viewport(),
        LayoutSettings::default(),
        None,
        0,
        Arc::clone(&probe) as Arc<dyn LayoutEvents>,
    )
    .expect("session opens");

    // Before the worker has published anything the honest answer is 0,
    // never an error — and the probe is armed only after the handle is
    // parked, so the count it reads is the mid-layout one.
    *probe.session.lock().expect("session lock") = Some(Arc::downgrade(&session));
    gate_tx.send(()).expect("gate send");

    wait_for(&rx, |e| matches!(e, Event::FirstPage(0, 0)));
    assert_eq!(
        probe.observed.load(Ordering::Acquire),
        1,
        "exactly one page is published when first_page_ready fires"
    );

    let page_count = wait_chapter_ready(&rx, 0, 0);
    assert!(page_count > 1, "fixture must span multiple pages");
    assert_eq!(
        session.published_page_count(0),
        page_count,
        "the complete chapter publishes every page"
    );
    assert_eq!(
        session.chapter(0).expect("geometry").page_count,
        session.published_page_count(0),
        "and agrees with ChapterGeometry"
    );
}

/// Unstarted means 0, and asking must not change that: unlike
/// `chapter()`/`page()`, this query neither schedules the chapter nor
/// moves the prefetch focus onto it. Proven by asking about a chapter
/// outside the opening prefetch radius and watching it stay unlaid while
/// the worker finishes everything it WAS given.
#[test]
fn published_page_count_is_zero_for_unstarted_chapter_and_never_schedules() {
    let dir = TempDir::new().expect("tempdir");
    let doc = cjk_doc(2);
    let docs: Vec<&str> = (0..6).map(|_| doc.as_str()).collect();
    let path = book(&dir, &docs);
    let (session, rx) = open(&path, viewport(), 0);

    // Chapter 5 is outside the opening chapter's prefetch radius (±2).
    for _ in 0..5 {
        assert_eq!(session.published_page_count(5), 0);
    }
    // Drain the work the session DID schedule, so the worker has had
    // every chance to pick chapter 5 up if the query had queued it.
    for spine_idx in 0..=2 {
        wait_chapter_ready(&rx, 0, spine_idx);
    }
    assert_eq!(
        session.published_page_count(5),
        0,
        "asking must not have scheduled chapter 5"
    );
    assert!(!session.is_ready(5));

    // The contrast: a query that DOES schedule gets it laid out.
    assert!(matches!(session.chapter(5), Err(EngineError::NotReady)));
    wait_chapter_ready(&rx, 0, 5);
    assert!(session.published_page_count(5) > 0);
}

/// Every remaining edge answers 0 rather than throwing: out-of-range
/// spine indexes, a chapter that failed closed, and a closed session.
#[test]
fn published_page_count_answers_zero_instead_of_throwing() {
    let dir = TempDir::new().expect("tempdir");
    let good = cjk_doc(2);
    let path = book(&dir, &[&good, "no root element here at all"]);
    let (session, rx) = open(&path, viewport(), 0);

    assert_eq!(session.published_page_count(99), 0, "out of range");

    wait_chapter_ready(&rx, 0, 0);
    wait_for(&rx, |e| matches!(e, Event::ChapterFailed(0, 1)));
    assert!(matches!(
        session.chapter(1),
        Err(EngineError::UnsupportedContent { .. })
    ));
    assert_eq!(
        session.published_page_count(1),
        0,
        "a failed chapter has no published pages"
    );

    assert!(session.published_page_count(0) > 0);
    session.close();
    assert_eq!(
        session.published_page_count(0),
        0,
        "a closed session publishes nothing"
    );
}

/// A book made fixed-layout only by its itemrefs' own
/// `rendition:layout-pre-paginated` properties — no package-level
/// `rendition:layout` meta anywhere — still fails at open with the
/// degradation contract's exact detail string.
#[test]
fn itemref_only_fixed_layout_rejected_at_open() {
    let dir = TempDir::new().expect("tempdir");
    let path = build_epub(
        &dir,
        EpubBuilder::new()
            .resource("ch01.xhtml", "application/xhtml+xml", LATIN_DOC.as_bytes())
            .spine(&["ch01.xhtml"])
            .itemref_properties("rendition:layout-pre-paginated"),
    );
    let (tx, _rx) = channel();
    match EngineSession::open(
        &path,
        registry(),
        viewport(),
        LayoutSettings::default(),
        None,
        0,
        Arc::new(TestEvents(tx)),
    ) {
        Err(EngineError::UnsupportedContent { detail }) => assert_eq!(detail, "fixed-layout"),
        other => panic!("expected UnsupportedContent, got {:?}", other.map(|_| ())),
    }
}
