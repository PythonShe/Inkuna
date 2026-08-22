use super::{project, Projection};
use crate::dom::parse;
use crate::style::{parse_sheet, resolve};
use crate::test_support::{CJK_HORIZONTAL_DOC, CJK_VERTICAL_RUBY_DOC};

/// Parses, resolves against the given sheets, projects.
fn projected(xhtml: &str, css: &[&str]) -> Projection {
    let doc = parse(xhtml.as_bytes()).unwrap();
    let sheets: Vec<_> = css.iter().map(|sheet| parse_sheet(sheet)).collect();
    let styled = resolve(&doc, &sheets);
    project(&styled)
}

/// The chars of `text` within a span's range.
fn slice(projection: &Projection, range: &std::ops::Range<u64>) -> String {
    projection
        .text
        .chars()
        .skip(range.start as usize)
        .take((range.end - range.start) as usize)
        .collect()
}

#[test]
fn excludes_rt_includes_base() {
    let projection = projected(CJK_VERTICAL_RUBY_DOC, &[]);
    assert_eq!(projection.text, "東京の空は高かった。");
    assert!(!projection.text.contains("とうきょう"));
    assert!(!projection.text.contains("そら"));

    // Contiguous spans: 東京 / の / 空 / は高かった。
    let ranges: Vec<_> = projection.spans.iter().map(|s| s.char_range.clone()).collect();
    assert_eq!(ranges, vec![0..2, 2..3, 3..4, 4..10]);
    for span in &projection.spans {
        assert_eq!(span.node_char_start, 0);
    }
    assert_eq!(projection.char_len, 10);
}

#[test]
fn display_none_excluded() {
    let projection = projected(
        r#"<html><body>
<p>kept one</p>
<div class="hidden"><p>dropped</p></div>
<p>kept two</p>
</body></html>"#,
        &[".hidden { display: none }"],
    );
    assert_eq!(projection.text, "kept one\nkept two");
}

#[test]
fn whitespace_collapse_matches_rules() {
    let projection = projected(
        "<html><body><p>a\t\n  b\u{A0}\u{A0}c</p><p>\u{A0} d  </p></body></html>",
        &[],
    );
    assert_eq!(projection.text, "a b c\nd");
}

#[test]
fn soft_hyphen_dropped() {
    let projection = projected(
        "<html><body><p>hy\u{AD}phen and hy\u{AD} phen</p></body></html>",
        &[],
    );
    assert_eq!(projection.text, "hyphen and hy phen");
}

#[test]
fn offsets_are_scalar_counts() {
    let projection = projected(
        "<html><body><p>𠀋a <em>𠀋</em>b</p></body></html>",
        &[],
    );
    assert_eq!(projection.text, "𠀋a 𠀋b");
    // 5 scalars, though the bytes say otherwise.
    assert_eq!(projection.char_len, 5);
    assert_eq!(projection.char_len, projection.text.chars().count() as u64);

    let texts: Vec<String> = projection
        .spans
        .iter()
        .map(|span| slice(&projection, &span.char_range))
        .collect();
    assert_eq!(texts, vec!["𠀋a", "𠀋", "b"]);
}

#[test]
fn anchor_offsets() {
    let projection = projected(
        r#"<html><body>
<p id="first">alpha</p>
<div id="empty"></div>
<p id="second">beta</p>
<p id="tail"></p>
</body></html>"#,
        &[],
    );
    assert_eq!(projection.text, "alpha\nbeta");
    assert_eq!(
        projection.anchors,
        vec![
            ("first".to_string(), 0),
            // The empty div anchors at the next projected char…
            ("empty".to_string(), 6),
            ("second".to_string(), 6),
            // …and a trailing empty element at char_len.
            ("tail".to_string(), 10),
        ]
    );
}

/// Pins the canonical nested-block rule: a block boundary breaks at
/// START and END, deliberately deviating from the legacy
/// `extract_spine_text` collapse (see the module doc). The legacy stream
/// dies in Movement 6 and all corpus offsets rebaseline onto this.
#[test]
fn nested_block_boundaries_break_at_start_and_end() {
    let nested_div = projected("<html><body><div>a<p>b</p></div></body></html>", &[]);
    assert_eq!(nested_div.text, "a\nb");

    let list_item = projected("<html><body><li>Item <p>para</p></li></body></html>", &[]);
    assert_eq!(list_item.text, "Item\npara");

    let stray = projected("<html><body><p>a</p>stray<p>b</p></body></html>", &[]);
    assert_eq!(stray.text, "a\nstray\nb");
}

#[test]
fn br_emits_newline() {
    let projection = projected(
        "<html><body><p>line one<br/>line two</p></body></html>",
        &[],
    );
    assert_eq!(projection.text, "line one\nline two");
}

#[test]
fn projection_snapshot_cjk() {
    let projection = projected(CJK_HORIZONTAL_DOC, &[]);
    assert_eq!(
        projection.text,
        "第一章\n月光洒在窗台上，屋里一片寂静。\n他放下手中的书，望向远处的群山。"
    );
    assert!(!projection.truncated);
    assert_eq!(projection.anchors, vec![("chapter-one".to_string(), 0)]);
}
