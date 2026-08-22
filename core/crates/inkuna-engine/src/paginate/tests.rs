use std::ops::Range;
use std::path::Path;
use std::sync::{Arc, OnceLock};

use crate::dom::{parse, StylesheetSource};
use crate::fixed::Fx;
use crate::fonts::FontRegistry;
use crate::settings::{LayoutSettings, Typography};
use crate::style::{parse_sheet, resolve};
use crate::test_support::tiny_png;
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
    run_with(doc_src, viewport, "OEBPS/ch01.xhtml", &|_| None)
}

fn run_with(
    doc_src: &str,
    viewport: FxSize,
    resource_path: &str,
    resources: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> (Vec<LaidPage>, ChapterLayoutResult, String) {
    run_typo(doc_src, viewport, resource_path, resources, &|_| {})
}

/// Like [`run_with`] but lets the test tweak the resolved `Typography`
/// (e.g. a nonzero `paragraph_indent`, which no setting produces).
fn run_typo(
    doc_src: &str,
    viewport: FxSize,
    resource_path: &str,
    resources: &dyn Fn(&str) -> Option<Vec<u8>>,
    tweak: &dyn Fn(&mut Typography),
) -> (Vec<LaidPage>, ChapterLayoutResult, String) {
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
    let mut typography = settings.typography();
    tweak(&mut typography);
    let input = ChapterInput {
        styled: &styled,
        projection: &projection,
        fonts: registry(),
        typography: &typography,
        settings: &settings,
        viewport,
        lang: None,
        resource_path,
        resources,
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

/// The paragraph range whose text (separator trimmed) equals `wanted`.
fn para_range_of(text: &str, paras: &[Range<u64>], wanted: &str) -> Range<u64> {
    let chars: Vec<char> = text.chars().collect();
    paras
        .iter()
        .find(|r| {
            chars[r.start as usize..r.end as usize]
                .iter()
                .collect::<String>()
                .trim_end_matches('\n')
                == wanted
        })
        .expect("paragraph found")
        .clone()
}

/// The placed first line of the paragraph starting at `range.start`.
fn first_line<'a>(pages: &'a [LaidPage], range: &Range<u64>) -> &'a super::PlacedLine {
    pages
        .iter()
        .flat_map(|p| p.lines.iter())
        .find(|pl| pl.line.char_range.start == range.start)
        .expect("paragraph first line")
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
fn heading_never_stranded_before_three_line_paragraph() {
    // A <=3-line paragraph never splits (min 2 lines on BOTH sides), so
    // keeping the heading demands heading + ALL THREE follower lines —
    // a keep of only 2 would let the follower flush itself to the next
    // page and strand the heading at the page bottom. Sweep the filler
    // height so the pair lands at every position near the bottom.
    let heading = "見出しの題";
    let sentence = "月光洒在窗台上屋里一片寂静。";
    let follower = sentence.repeat(3);
    let mut moved = false;
    for fillers in 0..12 {
        let mut body = String::new();
        for _ in 0..fillers {
            body.push_str(&format!("<p>{sentence}</p>\n"));
        }
        body.push_str(&format!("<h2>{heading}</h2>\n"));
        body.push_str(&format!("<p>{follower}</p>\n"));
        let (pages, result, text) = run(&wrap_body(&body, ""), viewport(300.0, 400.0));
        assert_partition(&pages, &result);
        let paras = paragraph_ranges(&text);
        let heading_range = para_range_of(&text, &paras, heading);
        let follow_range = para_range_of(&text, &paras, &follower);
        let total: usize = pages.iter().map(|p| para_lines_on(p, &follow_range)).sum();
        assert_eq!(total, 3, "fixture: the follower breaks into exactly 3 lines");
        let heading_page = pages
            .iter()
            .position(|p| para_lines_on(p, &heading_range) > 0)
            .expect("heading placed");
        if heading_page > 0 {
            moved = true;
        }
        assert_eq!(
            para_lines_on(&pages[heading_page], &follow_range),
            3,
            "heading moves with its whole unsplittable follower (fillers={fillers})"
        );
    }
    assert!(moved, "some filler height pushed the heading to a later page");
}

#[test]
fn heading_keep_extends_to_image() {
    // A heading followed by an image (not a paragraph) gets the same
    // keep: the image's fitted extent is the follower's minimum
    // placeable unit, so the heading is never stranded above a page
    // break the image falls over.
    let heading = "見出しの題";
    let sentence = "月光洒在窗台上屋里一片寂静。";
    let resources = |_: &str| Some(tiny_png(200, 200));
    let mut moved = false;
    for fillers in 0..12 {
        let mut body = String::new();
        for _ in 0..fillers {
            body.push_str(&format!("<p>{sentence}</p>\n"));
        }
        body.push_str(&format!("<h2>{heading}</h2>\n<img src=\"plate.png\"/>"));
        let (pages, result, text) = run_with(
            &wrap_body(&body, ""),
            viewport(300.0, 400.0),
            "OEBPS/ch01.xhtml",
            &resources,
        );
        assert_partition(&pages, &result);
        let paras = paragraph_ranges(&text);
        let heading_range = para_range_of(&text, &paras, heading);
        let heading_page = pages
            .iter()
            .position(|p| para_lines_on(p, &heading_range) > 0)
            .expect("heading placed");
        if heading_page > 0 {
            moved = true;
        }
        assert_eq!(
            pages[heading_page].images.len(),
            1,
            "the image stays with its heading (fillers={fillers})"
        );
    }
    assert!(moved, "some filler height pushed the heading to a later page");
}

#[test]
fn indented_first_line_stays_in_content_box() {
    // The transcribed paragraph_indent is 0.0; force a nonzero one. The
    // first line must be broken AND justified against measure - indent,
    // so the indent shift never pushes it past the content box.
    let indent = 24.0;
    let sentence = "月光洒在窗台上屋里一片寂静。";
    let body = format!("<p>{}</p>\n", sentence.repeat(8));
    let (pages, result, _) = run_typo(
        &wrap_body(&body, ""),
        viewport(300.0, 400.0),
        "OEBPS/ch01.xhtml",
        &|_| None,
        &|t| t.paragraph_indent = indent,
    );
    assert_partition(&pages, &result);
    let content_x = Fx::from_pt(26.0);
    let content_right = Fx::from_pt(300.0 - 26.0);
    let content_w = content_right - content_x;
    let lines = &pages[0].lines;
    assert!(lines.len() >= 3, "got {} lines", lines.len());
    let first = &lines[0];
    assert_eq!(first.x, content_x + Fx::from_pt(indent), "first line shifted by the indent");
    assert_eq!(
        first.line.inline_extent,
        content_w - Fx::from_pt(indent),
        "first line justified to measure - indent"
    );
    let second = &lines[1];
    assert_eq!(second.x, content_x, "later lines carry no indent");
    assert_eq!(second.line.inline_extent, content_w, "later lines justify to the full measure");
    for page in &pages {
        for pl in &page.lines {
            assert!(
                pl.x + pl.line.inline_extent <= content_right,
                "no line exceeds the content box"
            );
        }
    }
}

#[test]
fn indent_applies_to_body_paragraphs_only() {
    let prose = "正文段落首行缩进正文段落。";
    let heading = "見出しの題";
    let quoted = "引用段落不加首行缩进引用。";
    let listed = "列表项目不加首行缩进列表。";
    let body = format!(
        "<p>{prose}</p>\n<h2>{heading}</h2>\n<blockquote>{quoted}</blockquote>\n<ul><li>{listed}</li></ul>\n"
    );
    let (pages, result, text) = run_typo(
        &wrap_body(&body, ""),
        viewport(300.0, 400.0),
        "OEBPS/ch01.xhtml",
        &|_| None,
        &|t| t.paragraph_indent = 24.0,
    );
    assert_partition(&pages, &result);
    let paras = paragraph_ranges(&text);
    let margin = Fx::from_pt(26.0);
    let indent = Fx::from_pt(24.0);
    let quote_inset = Fx::from_pt(17.0 * 1.5);
    let line_of = |wanted: &str| first_line(&pages, &para_range_of(&text, &paras, wanted));
    assert_eq!(line_of(prose).x, margin + indent, "body paragraph indents");
    assert_eq!(line_of(heading).x, margin, "heading gets no indent");
    assert_eq!(
        line_of(quoted).x,
        margin + quote_inset,
        "blockquote text gets its inset but no indent"
    );
    assert_eq!(line_of(listed).x, margin, "list item gets no indent");
}

#[test]
fn image_sniffed_once_per_href() {
    use std::cell::Cell;
    let calls = Cell::new(0usize);
    let resources = |href: &str| -> Option<Vec<u8>> {
        assert_eq!(href, "OEBPS/plate.png");
        calls.set(calls.get() + 1);
        Some(tiny_png(120, 80))
    };
    // The heading-keep lookahead measures the image, placement measures
    // it again, and the href repeats — still one fetch+sniff per pass.
    let body = "<h2>見出しの題</h2><img src=\"plate.png\"/><img src=\"plate.png\"/>";
    let (pages, _, _) = run_with(
        &wrap_body(body, ""),
        viewport(452.0, 600.0),
        "OEBPS/ch01.xhtml",
        &resources,
    );
    assert_eq!(calls.get(), 1, "one fetch+sniff per unique href per pagination pass");
    assert_eq!(pages.iter().map(|p| p.images.len()).sum::<usize>(), 2);
}

#[test]
fn code_block_forces_start_alignment() {
    let sentence = "月光洒在窗台上屋里一片寂静。";
    let prose = sentence.repeat(4);
    let code = "代码段落不做两端对齐处理。".repeat(4);
    let mixed_body = sentence.repeat(4);
    let body = format!(
        "<p>{prose}</p>\n<div><code>{code}</code></div>\n<p><code>行内</code>{mixed_body}</p>\n"
    );
    let (pages, result, text) = run(&wrap_body(&body, ""), viewport(300.0, 480.0));
    assert_partition(&pages, &result);
    let paras = paragraph_ranges(&text);
    let content_w = Fx::from_pt(300.0 - 2.0 * 26.0);
    let line_of = |wanted: &str| first_line(&pages, &para_range_of(&text, &paras, wanted));
    // The plain paragraph justifies, proving the fixture would justify.
    assert_eq!(line_of(&prose).line.inline_extent, content_w, "prose justifies");
    // A <code> holding the whole paragraph is a code block: Start
    // align, never justified — same as <pre>.
    assert!(
        line_of(&code).line.inline_extent < content_w,
        "code-block paragraph stays ragged"
    );
    // Inline code inside a prose paragraph does not kill justification.
    let mixed_text = format!("行内{mixed_body}");
    assert_eq!(
        line_of(&mixed_text).line.inline_extent,
        content_w,
        "inline code keeps the paragraph justified"
    );
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

#[test]
fn image_scaled_to_fit_never_upscaled() {
    // Content box 400 pt wide (452 minus the default 26 pt margins).
    let body = r#"<p>before</p><img src="small.png"/><img src="big.png"/>"#;
    let resources = |href: &str| -> Option<Vec<u8>> {
        match href {
            "OEBPS/small.png" => Some(tiny_png(100, 50)),
            "OEBPS/big.png" => Some(tiny_png(1000, 500)),
            _ => None,
        }
    };
    let (pages, result, _) = run_with(
        &wrap_body(body, ""),
        viewport(452.0, 600.0),
        "OEBPS/ch01.xhtml",
        &resources,
    );
    assert_eq!(result.page_count, 1);
    let images = &pages[0].images;
    assert_eq!(images.len(), 2);
    let small = &images[0];
    assert_eq!(small.href, "OEBPS/small.png");
    assert_eq!(small.frame.w, Fx::from_pt(100.0), "never upscaled");
    assert_eq!(small.frame.h, Fx::from_pt(50.0));
    assert_eq!(small.frame.x, Fx::from_pt(26.0 + 150.0), "inline-centered");
    let big = &images[1];
    assert_eq!(big.frame.w, Fx::from_pt(400.0), "fit to the content width");
    assert_eq!(big.frame.h, Fx::from_pt(200.0), "aspect preserved");
    assert_eq!(big.frame.x, Fx::from_pt(26.0));
}

#[test]
fn unsniffable_gets_placeholder() {
    let body = r#"<p>before</p><img src="junk.bin"/>"#;
    let resources = |_: &str| Some(b"not an image at all".to_vec());
    let (pages, _, _) = run_with(
        &wrap_body(body, ""),
        viewport(452.0, 600.0),
        "OEBPS/ch01.xhtml",
        &resources,
    );
    let img = &pages[0].images[0];
    assert_eq!(img.href, "OEBPS/junk.bin", "href still emitted");
    assert_eq!(img.frame.w, Fx::from_pt(160.0));
    assert_eq!(img.frame.h, Fx::from_pt(160.0));
}

#[test]
fn oversize_dimensions_get_placeholder() {
    let body = r#"<p>before</p><img src="huge.png"/>"#;
    let resources = |_: &str| Some(tiny_png(60_000, 100));
    let (pages, _, _) = run_with(
        &wrap_body(body, ""),
        viewport(452.0, 600.0),
        "OEBPS/ch01.xhtml",
        &resources,
    );
    let img = &pages[0].images[0];
    assert_eq!(img.frame.w, Fx::from_pt(160.0), "claimed 60000 px -> placeholder");
    assert_eq!(img.frame.h, Fx::from_pt(160.0));
}

#[test]
fn image_breaks_to_next_page() {
    // ~10 lines of filler leave less than 200 pt on the 348 pt page.
    let sentence = "月光洒在窗台上屋里一片寂静。";
    let body = format!("<p>{}</p><img src=\"plate.png\"/>", sentence.repeat(10));
    let resources = |_: &str| Some(tiny_png(200, 200));
    let (pages, result, _) = run_with(
        &wrap_body(&body, ""),
        viewport(300.0, 400.0),
        "OEBPS/ch01.xhtml",
        &resources,
    );
    assert!(result.page_count >= 2);
    assert!(pages[0].images.is_empty(), "no room on the first page");
    let img = &pages[1].images[0];
    assert_eq!(img.frame.y, Fx::from_pt(26.0), "lands at the next page's top");
    assert_eq!(img.frame.h, Fx::from_pt(200.0));
}

#[test]
fn image_href_normalized() {
    let body = r#"<p>before</p><img src="../images/a%20b.png"/>"#;
    let resources = |href: &str| -> Option<Vec<u8>> {
        assert_eq!(href, "OEBPS/images/a b.png", "lookup uses the resolved path");
        Some(tiny_png(50, 50))
    };
    let (pages, _, _) = run_with(
        &wrap_body(body, ""),
        viewport(452.0, 600.0),
        "OEBPS/text/ch1.xhtml",
        &resources,
    );
    let img = &pages[0].images[0];
    assert_eq!(img.href, "OEBPS/images/a b.png");
    assert_eq!(img.frame.w, Fx::from_pt(50.0));
}
