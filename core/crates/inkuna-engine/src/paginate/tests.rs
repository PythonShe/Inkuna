use std::ops::Range;
use std::path::Path;
use std::sync::{Arc, OnceLock};

use crate::dom::{parse, StylesheetSource};
use crate::fixed::Fx;
use crate::fonts::FontRegistry;
use crate::settings::LayoutSettings;
use crate::style::{parse_sheet, resolve};
use crate::text::project;

use super::{paginate, ChapterInput, ChapterLayoutResult, FxSize, LaidPage};

fn registry() -> &'static FontRegistry {
    static REG: OnceLock<Arc<FontRegistry>> = OnceLock::new();
    REG.get_or_init(|| {
        let dir = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../assets/fonts"
        ));
        FontRegistry::load(dir).expect("repo font set must load")
    })
}

/// Parses, styles, projects, and paginates one document at default
/// settings, returning the emitted pages, the result, and the
/// projection text (for locating paragraph boundaries).
fn run(doc_src: &str, viewport: FxSize) -> (Vec<LaidPage>, ChapterLayoutResult, String) {
    let doc = parse(doc_src.as_bytes()).expect("fixture parses");
    let css: Vec<&str> = doc
        .stylesheets
        .iter()
        .filter_map(|s| match s {
            StylesheetSource::Inline(text) => Some(text.as_str()),
            StylesheetSource::Linked(_) => None,
        })
        .collect();
    let sheets: Vec<_> = css.iter().map(|c| parse_sheet(c)).collect();
    let styled = resolve(&doc, &sheets);
    let projection = project(&styled);
    let settings = LayoutSettings::default();
    let typography = settings.typography();
    let input = ChapterInput {
        styled: &styled,
        projection: &projection,
        fonts: registry(),
        typography: &typography,
        settings: &settings,
        viewport,
        lang: None,
    };
    let mut pages = Vec::new();
    let result = paginate(&input, &mut |p| pages.push(p)).expect("paginate is infallible");
    (pages, result, projection.text)
}

fn viewport(w: f64, h: f64) -> FxSize {
    FxSize {
        width: Fx::from_pt(w),
        height: Fx::from_pt(h),
    }
}

fn wrap_body(body: &str, style: &str) -> String {
    format!(
        r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>t</title>{style}</head><body>
{body}
</body></html>"#
    )
}

fn cjk_doc(paras: usize) -> String {
    let mut body = String::new();
    for _ in 0..paras {
        body.push_str("<p>月光洒在窗台上，屋里一片寂静。他放下手中的书，望向远处的群山。</p>\n");
    }
    wrap_body(&body, "")
}

fn vertical_doc(paras: usize) -> String {
    let mut body = String::new();
    for _ in 0..paras {
        body.push_str(
            "<p><ruby>東京<rt>とうきょう</rt></ruby>の<ruby>空<rt>そら</rt></ruby>は高かった。</p>\n",
        );
    }
    wrap_body(&body, "<style>body { writing-mode: vertical-rl; }</style>")
}

/// Paragraph char ranges as pagination assigns them: each projection
/// stretch plus its trailing separator `\n`.
fn paragraph_ranges(text: &str) -> Vec<Range<u64>> {
    let mut ranges = Vec::new();
    let mut start = 0u64;
    let mut at = 0u64;
    for ch in text.chars() {
        at += 1;
        if ch == '\n' {
            ranges.push(start..at);
            start = at;
        }
    }
    if at > start || ranges.is_empty() {
        ranges.push(start..at);
    }
    ranges
}

/// The pages' char ranges are ascending, contiguous, and partition
/// `0..char_len` — every char on exactly one page.
fn assert_partition(pages: &[LaidPage], result: &ChapterLayoutResult) {
    assert_eq!(pages.len() as u32, result.page_count);
    let mut cursor = 0u64;
    for (i, page) in pages.iter().enumerate() {
        assert_eq!(page.index, i as u32);
        assert_eq!(page.char_range.start, cursor, "contiguous at page {i}");
        assert!(page.char_range.end >= page.char_range.start);
        cursor = page.char_range.end;
    }
    assert_eq!(cursor, result.char_len, "pages cover the laid prefix");
}

/// Lines of `page` that belong to the paragraph `range`.
fn para_lines_on(page: &LaidPage, range: &Range<u64>) -> usize {
    page.lines
        .iter()
        .filter(|pl| pl.line.char_range.start >= range.start && pl.line.char_range.end <= range.end)
        .count()
}

#[test]
fn progressive_emission_order() {
    let (pages, result, text) = run(&cjk_doc(14), viewport(300.0, 400.0));
    assert!(result.page_count > 1, "multi-page chapter");
    assert!(!result.truncated);
    assert_eq!(result.char_len, text.chars().count() as u64);
    assert_partition(&pages, &result);
}

#[test]
fn every_char_on_exactly_one_page() {
    // Horizontal fixture.
    let (pages, result, text) = run(&cjk_doc(10), viewport(280.0, 320.0));
    assert!(result.page_count > 1);
    assert_eq!(result.char_len, text.chars().count() as u64);
    assert_partition(&pages, &result);
    // Vertical fixture.
    let (pages, result, text) = run(&vertical_doc(12), viewport(420.0, 180.0));
    assert!(result.page_count > 1);
    assert_eq!(result.char_len, text.chars().count() as u64);
    assert_partition(&pages, &result);
}

#[test]
fn widow_orphan_min_two() {
    // Short (~3-line) paragraphs around one long paragraph; the long
    // one must split leaving >= 2 lines on both sides of every break,
    // and no <= 3-line paragraph ever splits.
    let sentence = "月光洒在窗台上屋里一片寂静。";
    let mut body = String::new();
    for i in 0..8 {
        let n = if i == 3 { 20 } else { 3 };
        body.push_str(&format!("<p>{}</p>\n", sentence.repeat(n)));
    }
    let (pages, result, text) = run(&wrap_body(&body, ""), viewport(300.0, 400.0));
    assert!(result.page_count > 1);
    assert_partition(&pages, &result);
    let paras = paragraph_ranges(&text);
    let mut any_split = false;
    for range in &paras {
        let counts: Vec<usize> = pages
            .iter()
            .map(|p| para_lines_on(p, range))
            .filter(|&n| n > 0)
            .collect();
        let total: usize = counts.iter().sum();
        if counts.len() > 1 {
            any_split = true;
            for &n in &counts {
                assert!(n >= 2, "a split leaves >= 2 lines on both sides: {counts:?}");
            }
        }
        if total <= 3 {
            assert_eq!(counts.len(), 1, "a {total}-line paragraph never splits");
        }
    }
    assert!(any_split, "the long paragraph split across pages");
}

#[test]
fn heading_keeps_two_lines() {
    // Sweep the filler height so the heading lands at every position
    // near the page bottom: whatever page holds the heading must also
    // hold at least two lines of the paragraph that follows it.
    let heading = "見出しの題";
    let sentence = "月光洒在窗台上屋里一片寂静。";
    let mut moved = false;
    for fillers in 0..12 {
        let mut body = String::new();
        for _ in 0..fillers {
            body.push_str(&format!("<p>{sentence}</p>\n"));
        }
        body.push_str(&format!("<h2>{heading}</h2>\n"));
        body.push_str(&format!("<p>{}</p>\n", sentence.repeat(6)));
        let (pages, result, text) = run(&wrap_body(&body, ""), viewport(300.0, 400.0));
        assert_partition(&pages, &result);
        let paras = paragraph_ranges(&text);
        let heading_at = paras
            .iter()
            .position(|r| {
                let chars: Vec<char> = text.chars().collect();
                let slice: String = chars[r.start as usize..r.end as usize]
                    .iter()
                    .collect();
                slice.trim_end_matches('\n') == heading
            })
            .expect("heading paragraph found");
        let heading_range = &paras[heading_at];
        let follow_range = &paras[heading_at + 1];
        let heading_page = pages
            .iter()
            .position(|p| para_lines_on(p, heading_range) > 0)
            .expect("heading placed");
        if heading_page > 0 {
            moved = true;
        }
        assert!(
            para_lines_on(&pages[heading_page], follow_range) >= 2,
            "heading keeps two lines of the next block (fillers={fillers})"
        );
    }
    assert!(moved, "some filler height pushed the heading to a later page");
}

#[test]
fn vertical_rl_axis_swap() {
    let (pages, result, _) = run(&vertical_doc(10), viewport(500.0, 160.0));
    assert!(result.page_count > 1, "got {} pages", result.page_count);
    let first = &pages[0];
    assert!(first.lines.len() >= 2);
    let first_x = first.lines.first().expect("lines").x;
    let last_x = first.lines.last().expect("lines").x;
    assert!(
        first_x > last_x,
        "columns advance right-to-left: {first_x:?} vs {last_x:?}"
    );
    // Inline origins sit at the content top, not the left margin.
    for pl in &first.lines {
        assert_eq!(pl.y, Fx::from_pt(26.0), "column inline origin at top");
    }
}

#[test]
fn page_cap_truncates() {
    // A degenerate viewport fits one overflowing one-char line per
    // page; five 4000-char paragraphs exceed the page cap.
    let mut body = String::new();
    for _ in 0..5 {
        body.push_str(&format!("<p>{}</p>\n", "永".repeat(4000)));
    }
    let (pages, result, text) = run(&wrap_body(&body, ""), viewport(60.0, 60.0));
    assert!(result.truncated);
    assert_eq!(result.page_count, super::MAX_PAGES_PER_CHAPTER);
    assert_eq!(pages.len() as u32, super::MAX_PAGES_PER_CHAPTER);
    assert!(result.char_len < text.chars().count() as u64);
    assert_eq!(
        result.char_len,
        pages.last().expect("pages").char_range.end,
        "char_len is the retained prefix"
    );
    assert_partition(&pages, &result);
}

#[test]
fn blockquote_inset_narrower_lines() {
    let sentence = "月光洒在窗台上屋里一片寂静。";
    let body = format!(
        "<p>{0}</p>\n<blockquote><p>{0}</p></blockquote>\n",
        sentence.repeat(4)
    );
    let (pages, result, text) = run(&wrap_body(&body, ""), viewport(300.0, 500.0));
    assert_eq!(result.page_count, 1);
    let paras = paragraph_ranges(&text);
    let page = &pages[0];
    let line_of = |range: &Range<u64>| {
        page.lines
            .iter()
            .find(|pl| pl.line.char_range.start == range.start)
            .expect("paragraph first line")
    };
    let plain = line_of(&paras[0]);
    let quoted = line_of(&paras[1]);
    let inset = Fx::from_pt(17.0 * 1.5);
    assert_eq!(quoted.x, plain.x + inset, "1.5 em inset on the inline start");
    // Both first lines are justified to their (different) widths.
    let content_w = Fx::from_pt(300.0 - 2.0 * 26.0);
    assert_eq!(plain.line.inline_extent, content_w);
    assert_eq!(
        quoted.line.inline_extent,
        content_w - inset - inset,
        "quoted lines justify to the narrower measure"
    );
}
