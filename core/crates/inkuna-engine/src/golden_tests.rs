//! The golden corpus (determinism gate) and the adversarial suite.
//!
//! Each roster fixture (see `test_support::golden_roster`) is laid out
//! at the fixed 390×664 pt viewport under default settings and asserted
//! against COMMITTED text artifacts in `golden/`:
//!
//! - `<fixture>.pages.txt` — per-chapter page counts + per-page digests;
//! - `<fixture>.page0.txt` — the canonical-serialization dump of
//!   chapter 0 page 0 rendered as text lines;
//! - `<fixture>.projection.txt` — chapter 0's canonical projection.
//!
//! Run with `INKUNA_UPDATE_GOLDEN=1` to REWRITE the golden files instead
//! of asserting (then commit the diff deliberately — these digests are
//! the cross-platform parity contract plan 02's device harness compares
//! on real hardware).

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use inkuna_content::test_support::EpubBuilder;
use inkuna_content::read_package;
use tempfile::TempDir;

use crate::corpus::extract_corpus;
use crate::error::EngineError;
use crate::fonts::FontRegistry;
use crate::paginate::MAX_PAGES_PER_CHAPTER;
use crate::session::{EngineSession, LayoutEvents, Viewport};
use crate::test_support::{
    build_epub, golden_roster, golden_settings, tiny_png, update_golden, GOLDEN_VIEWPORT_PT,
    MALFORMED_DOC,
};

fn registry() -> Arc<FontRegistry> {
    static REG: OnceLock<Arc<FontRegistry>> = OnceLock::new();
    Arc::clone(REG.get_or_init(|| {
        let dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../assets/fonts"));
        FontRegistry::load(dir).expect("repo font set must load")
    }))
}

struct NoEvents;
impl LayoutEvents for NoEvents {
    fn first_page_ready(&self, _: u64, _: u32) {}
    fn chapter_ready(&self, _: u64, _: u32, _: u32) {}
}

fn open_session(path: &PathBuf) -> Arc<EngineSession> {
    EngineSession::open(
        path,
        registry(),
        Viewport {
            width: GOLDEN_VIEWPORT_PT.0,
            height: GOLDEN_VIEWPORT_PT.1,
        },
        golden_settings(),
        None,
        0,
        Arc::new(NoEvents),
    )
    .expect("session opens")
}

/// Waits until every chapter is laid out (or failed closed); queries
/// schedule misses, so polling drives the whole spine through the
/// worker.
fn wait_all(session: &EngineSession, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    for idx in 0..session.spine_len() {
        loop {
            match session.chapter(idx) {
                Ok(_) | Err(EngineError::UnsupportedContent { .. }) => break,
                Err(EngineError::NotReady) => {
                    assert!(
                        Instant::now() < deadline,
                        "chapter {idx} did not lay out within {timeout:?}"
                    );
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(e) => panic!("unexpected error waiting for chapter {idx}: {e}"),
            }
        }
    }
}

fn golden_dir() -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/golden")).to_path_buf()
}

/// Asserts `actual` against the committed golden file, or rewrites it
/// under `INKUNA_UPDATE_GOLDEN=1`.
fn check_golden(fixture: &str, artifact: &str, actual: &str) {
    let path = golden_dir().join(format!("{fixture}.{artifact}"));
    if update_golden() {
        std::fs::create_dir_all(golden_dir()).expect("golden dir");
        std::fs::write(&path, actual).expect("write golden file");
        return;
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "missing golden file {path:?} ({e}); generate with \
             INKUNA_UPDATE_GOLDEN=1 cargo test -p inkuna-engine golden"
        )
    });
    assert_eq!(
        committed, actual,
        "{fixture}.{artifact} drifted from the committed golden — if the \
         change is intentional, regenerate with INKUNA_UPDATE_GOLDEN=1 and \
         commit the diff"
    );
}

/// Per-chapter page counts + per-page digests.
fn render_pages(session: &EngineSession) -> String {
    let mut out = String::new();
    for idx in 0..session.spine_len() {
        let g = session.chapter(idx).expect("chapter geometry");
        out.push_str(&format!(
            "chapter {idx}: pages={} chars={} truncated={} writing_mode={:?} rtl={}\n",
            g.page_count, g.char_range.end, g.truncated, g.writing_mode, g.rtl_progression
        ));
        for p in 0..g.page_count {
            let digest = session.page_digest(idx, p).expect("page digest");
            out.push_str(&format!("  page {p}: {digest}\n"));
        }
    }
    out
}

/// Chapter 0 page 0's display list rendered as deterministic text
/// lines — the canonical serialization made human-diffable.
fn render_page0(session: &EngineSession) -> String {
    let list = session.page(0, 0).expect("chapter 0 page 0");
    let mut out = String::new();
    out.push_str(&format!("generation {}\n", list.generation));
    out.push_str(&format!("glyph_runs {}\n", list.glyph_runs.len()));
    for run in &list.glyph_runs {
        out.push_str(&format!(
            "run font={} size={:?} color={:?} orientation={:?}\n",
            run.font_id, run.size, run.color_role, run.orientation
        ));
        out.push_str("  ids");
        for id in &run.glyph_ids {
            out.push_str(&format!(" {id}"));
        }
        out.push('\n');
        out.push_str("  pos");
        for p in &run.positions {
            out.push_str(&format!(" {p:?}"));
        }
        out.push('\n');
    }
    out.push_str(&format!("images {}\n", list.images.len()));
    for img in &list.images {
        out.push_str(&format!("image href={} rect={:?}\n", img.href, img.rect));
    }
    out.push_str(&format!("decorations {}\n", list.decorations.len()));
    for d in &list.decorations {
        out.push_str(&format!(
            "decoration kind={:?} color={:?} rect={:?}\n",
            d.kind, d.color_role, d.rect
        ));
    }
    out.push_str(&format!("links {}\n", list.links.len()));
    for l in &list.links {
        out.push_str(&format!("link target={} rect={:?}\n", l.target, l.rect));
    }
    out.push_str(&format!("a11y {}\n", list.a11y.len()));
    for b in &list.a11y {
        out.push_str(&format!(
            "block role={:?} is_link={} lang={:?} rect={:?} text={:?}\n",
            b.role, b.is_link, b.lang, b.rect, b.text
        ));
    }
    out
}

/// The full golden gate for one roster fixture.
fn run_golden(name: &str) {
    let fixture = golden_roster()
        .into_iter()
        .find(|f| f.name == name)
        .expect("fixture in roster");
    let dir = TempDir::new().expect("tempdir");
    let path = build_epub(&dir, (fixture.build)());
    let session = open_session(&path);
    wait_all(&session, Duration::from_secs(120));

    check_golden(name, "pages.txt", &render_pages(&session));
    check_golden(name, "page0.txt", &render_page0(&session));

    let package = read_package(&path).expect("package reads");
    let spine: Vec<String> = package.spine.iter().map(|s| s.href.clone()).collect();
    let corpus = extract_corpus(&path, &spine, 32 * 1024 * 1024);
    let projection = corpus[0].as_deref().expect("chapter 0 projects");
    check_golden(name, "projection.txt", projection);
}

#[test]
fn digests_match_golden_latin() {
    run_golden("latin");
}

#[test]
fn digests_match_golden_cjk_horizontal() {
    run_golden("cjk_horizontal");
}

#[test]
fn digests_match_golden_cjk_vertical_ruby() {
    run_golden("cjk_vertical_ruby");
}

#[test]
fn digests_match_golden_rtl() {
    run_golden("rtl");
}

#[test]
fn digests_match_golden_mixed_script() {
    run_golden("mixed_script");
}

/// The RTL fixture must actually validate Hebrew shaping: before the
/// Hebrew fallback stage existed, 464/576 of page 0's glyphs were
/// `.notdef` and the golden locked in tofu — validating nothing.
#[test]
fn rtl_golden_shapes_hebrew_not_tofu() {
    let fixture = golden_roster()
        .into_iter()
        .find(|f| f.name == "rtl")
        .expect("rtl in roster");
    let dir = TempDir::new().expect("tempdir");
    let path = build_epub(&dir, (fixture.build)());
    let session = open_session(&path);
    wait_all(&session, Duration::from_secs(120));

    let list = session.page(0, 0).expect("page 0");
    let total: usize = list.glyph_runs.iter().map(|r| r.glyph_ids.len()).sum();
    let notdef: usize = list
        .glyph_runs
        .iter()
        .flat_map(|r| r.glyph_ids.iter())
        .filter(|&&g| g == 0)
        .count();
    assert!(total > 0, "page 0 must carry glyphs");
    assert!(
        notdef * 20 < total,
        "notdef must stay under 5% of page 0's glyphs: {notdef}/{total}"
    );
    // Distinct pages must hash distinctly — all-tofu (or all-identical)
    // pages collapsing to one digest would gut the parity gate.
    let d0 = session.page_digest(0, 0).expect("digest page 0");
    let d1 = session.page_digest(0, 1).expect("digest page 1");
    assert_ne!(d0, d1, "pages 0 and 1 must not share a digest");
}

#[test]
fn digests_match_golden_image_heavy() {
    run_golden("image_heavy");
}

#[test]
fn digests_match_golden_table_degradation() {
    run_golden("table_degradation");
}

// --- Adversarial suite ------------------------------------------------
//
// Hostile inputs must degrade exactly as specified — a truncated prefix
// or an UnsupportedContent scoped to the resource — with every budget
// holding: no panic, no unbounded allocation, page counts and char
// lengths under the caps.

fn one_chapter_book(dir: &TempDir, body: &[u8]) -> PathBuf {
    build_epub(
        dir,
        EpubBuilder::new()
            .resource("ch01.xhtml", "application/xhtml+xml", body)
            .spine(&["ch01.xhtml"]),
    )
}

fn wrap(body: &str) -> String {
    format!(
        r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>t</title></head><body>{body}</body></html>"#
    )
}

/// Budget assertions common to every adversarial layout that succeeds.
fn assert_budgets(session: &EngineSession, idx: u32) {
    let g = session.chapter(idx).expect("geometry");
    assert!(g.page_count <= MAX_PAGES_PER_CHAPTER);
    // The projection is capped by the 4 MiB text budget, so chars can
    // never exceed it.
    assert!(g.char_range.end <= 4 * 1024 * 1024);
}

#[test]
fn budget_bomb_nodes() {
    // 300k elements: the DOM truncates at its node budget and layout
    // renders the laid-out prefix.
    let mut body = String::with_capacity(310_000 * 13 + 64);
    body.push_str("<p>prefix text</p>");
    for _ in 0..300_000 {
        body.push_str("<span></span>");
    }
    body.push_str("<p>tail text past the budget</p>");
    let dir = TempDir::new().expect("tempdir");
    let path = one_chapter_book(&dir, wrap(&body).as_bytes());
    let session = open_session(&path);
    wait_all(&session, Duration::from_secs(120));
    let g = session.chapter(0).expect("truncated chapter still lays out");
    assert!(g.truncated, "node bomb must surface as truncation");
    assert!(session.page(0, 0).is_ok());
    assert_budgets(&session, 0);
}

#[test]
fn budget_bomb_attr() {
    // A multi-megabyte attribute value: capped at parse, layout
    // succeeds.
    let huge = "x".repeat(2 * 1024 * 1024);
    let body = format!(r#"<p class="{huge}">short text</p>"#);
    let dir = TempDir::new().expect("tempdir");
    let path = one_chapter_book(&dir, wrap(&body).as_bytes());
    let session = open_session(&path);
    wait_all(&session, Duration::from_secs(120));
    let g = session.chapter(0).expect("attr bomb chapter lays out");
    assert!(g.char_range.end > 0);
    assert_budgets(&session, 0);
}

#[test]
fn stylesheet_bomb() {
    // Linked CSS totalling past the 1 MiB budget: whole sheets drop
    // from the END of the cascade, earlier sheets stay intact. The
    // early small sheet hides one marker (its effect must survive);
    // the late huge sheet would hide another (its effect must NOT
    // apply, because the whole late sheet drops); layout succeeds
    // regardless.
    let early_css = ".early-gone { display: none; }";
    let mut late_css = String::with_capacity(2 * 1024 * 1024 + 64);
    late_css.push_str(".late-gone { display: none; } ");
    while late_css.len() < 2 * 1024 * 1024 {
        late_css.push_str(".c { text-align: center; } ");
    }
    let chapter = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>t</title>
<link rel="stylesheet" href="early.css"/>
<link rel="stylesheet" href="big.css"/></head>
<body><p class="early-gone">EARLYMARKER</p><p class="late-gone">LATEMARKER</p><p>styled text survives</p></body></html>"#;
    let dir = TempDir::new().expect("tempdir");
    let path = build_epub(
        &dir,
        EpubBuilder::new()
            .resource("ch01.xhtml", "application/xhtml+xml", chapter.as_bytes())
            .resource("early.css", "text/css", early_css.as_bytes())
            .resource("big.css", "text/css", late_css.as_bytes())
            .spine(&["ch01.xhtml"]),
    );
    let session = open_session(&path);
    wait_all(&session, Duration::from_secs(120));
    let g = session.chapter(0).expect("stylesheet bomb chapter lays out");
    assert!(g.char_range.end > 0);
    let text = session
        .text_range(
            0,
            crate::session::CharRange {
                start: 0,
                end: g.char_range.end,
            },
        )
        .expect("chapter text");
    assert!(
        !text.contains("EARLYMARKER"),
        "the early small sheet must survive the cap: {text:?}"
    );
    assert!(
        text.contains("LATEMARKER"),
        "the late huge sheet must drop whole: {text:?}"
    );
    let page = session.page(0, 0).expect("page 0");
    assert!(!page.glyph_runs.is_empty());
    assert_budgets(&session, 0);
}

#[test]
fn deep_nesting() {
    // 1000-deep nesting: flattened at the depth budget, text survives.
    let mut body = String::new();
    for _ in 0..1000 {
        body.push_str("<div>");
    }
    body.push_str("<p>deep text</p>");
    for _ in 0..1000 {
        body.push_str("</div>");
    }
    let dir = TempDir::new().expect("tempdir");
    let path = one_chapter_book(&dir, wrap(&body).as_bytes());
    let session = open_session(&path);
    wait_all(&session, Duration::from_secs(120));
    let g = session.chapter(0).expect("deep chapter lays out");
    assert!(g.char_range.end > 0);
    assert_budgets(&session, 0);
}

#[test]
fn absurd_image_header() {
    // A PNG header claiming 60k×60k: dimensions out of budget degrade
    // to the fixed placeholder box, never a huge allocation.
    let body = r#"<p>before</p><img src="huge.png" alt="huge"/><p>after</p>"#;
    let dir = TempDir::new().expect("tempdir");
    let path = build_epub(
        &dir,
        EpubBuilder::new()
            .resource("ch01.xhtml", "application/xhtml+xml", wrap(body).as_bytes())
            .resource("huge.png", "image/png", &tiny_png(60_000, 60_000))
            .spine(&["ch01.xhtml"]),
    );
    let session = open_session(&path);
    wait_all(&session, Duration::from_secs(120));
    let g = session.chapter(0).expect("chapter lays out");
    let mut found = false;
    for p in 0..g.page_count {
        let page = session.page(0, p).expect("page");
        for img in &page.images {
            assert_eq!(img.href, "OEBPS/huge.png");
            // The 160×160 pt placeholder box.
            assert_eq!(img.rect.width, 160.0);
            assert_eq!(img.rect.height, 160.0);
            found = true;
        }
    }
    assert!(found, "the placeholder image placement must be emitted");
    assert_budgets(&session, 0);
}

#[test]
fn malformed_xhtml_recovers() {
    let dir = TempDir::new().expect("tempdir");
    let path = one_chapter_book(&dir, MALFORMED_DOC.as_bytes());
    let session = open_session(&path);
    wait_all(&session, Duration::from_secs(120));
    let g = session.chapter(0).expect("malformed chapter recovers");
    let text = session
        .text_range(
            0,
            crate::session::CharRange {
                start: 0,
                end: g.char_range.end,
            },
        )
        .expect("text");
    assert!(text.contains("Fish & chips."));
    assert_budgets(&session, 0);
}

#[test]
fn zip_entry_bomb() {
    // A chapter entry inflating past the per-resource decompression cap
    // fails closed, scoped to the chapter; the book survives.
    let bomb = "a".repeat(17 * 1024 * 1024);
    let good = wrap("<p>good chapter</p>");
    let dir = TempDir::new().expect("tempdir");
    let path = build_epub(
        &dir,
        EpubBuilder::new()
            .resource("ch01.xhtml", "application/xhtml+xml", good.as_bytes())
            .resource("bomb.xhtml", "application/xhtml+xml", bomb.as_bytes())
            .spine(&["ch01.xhtml", "bomb.xhtml"]),
    );
    let session = open_session(&path);
    wait_all(&session, Duration::from_secs(120));
    assert!(session.chapter(0).is_ok());
    match session.chapter(1) {
        Err(EngineError::UnsupportedContent { .. }) => {}
        other => panic!("expected the bomb chapter to fail closed, got {other:?}"),
    }
    assert!(session.page(0, 0).is_ok(), "the book stays usable");
}
