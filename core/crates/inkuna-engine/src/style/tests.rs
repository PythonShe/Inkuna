use super::{
    parse_sheet, resolve, Direction, FontStyle, FontWeight, StyledDocument, TextAlign, WritingMode,
};
use crate::dom::{parse, Document, ElementName, NodeId, StylesheetSource};
use crate::test_support::{CJK_HORIZONTAL_DOC, RTL_DOC};

/// Parses the doc and resolves it against the given CSS sheets.
fn styled<'d>(doc: &'d Document, css: &[&str]) -> StyledDocument<'d> {
    let sheets: Vec<_> = css.iter().map(|sheet| parse_sheet(sheet)).collect();
    resolve(doc, &sheets)
}

/// First element with the given name.
fn find(doc: &Document, name: &ElementName) -> NodeId {
    (0..doc.nodes.len() as u32)
        .map(NodeId)
        .find(|&id| doc.element(id).is_some_and(|data| data.name == *name))
        .unwrap_or_else(|| panic!("no {name:?} in fixture"))
}

fn style_of(styled: &StyledDocument<'_>, id: NodeId) -> super::ComputedStyle {
    styled.styles[id.0 as usize]
}

#[test]
fn class_id_descendant_selectors_apply() {
    let doc = parse(
        br#"<html><body>
<div class="note aside"><p>plain <strong>marked</strong></p></div>
<p><strong>outside</strong></p>
</body></html>"#,
    )
    .unwrap();
    let styled = styled(&doc, &[".note strong { font-style: italic }"]);

    let inside = find(&doc, &ElementName::Strong);
    assert_eq!(style_of(&styled, inside).font_style, FontStyle::Italic);

    // The strong outside .note keeps the UA default.
    let outside = (0..doc.nodes.len() as u32)
        .map(NodeId)
        .filter(|&id| {
            doc.element(id)
                .is_some_and(|data| data.name == ElementName::Strong)
        })
        .nth(1)
        .unwrap();
    assert_eq!(style_of(&styled, outside).font_style, FontStyle::Normal);
    // Both are UA-bold either way.
    assert_eq!(style_of(&styled, outside).font_weight, FontWeight::Bold);
}

#[test]
fn specificity_and_order() {
    let doc = parse(br#"<html><body><p id="x" class="x">text</p></body></html>"#).unwrap();

    // #x (100) beats .x (10) beats p (1), whatever the source order.
    let styled_by_id = styled(
        &doc,
        &["#x { text-align: center } .x { text-align: end } p { text-align: start }"],
    );
    let p = find(&doc, &ElementName::P);
    assert_eq!(style_of(&styled_by_id, p).text_align, TextAlign::Center);

    let no_id = parse(br#"<html><body><p class="x">text</p></body></html>"#).unwrap();
    let styled_by_class = styled(
        &no_id,
        &["p { text-align: start } .x { text-align: end } "],
    );
    let p = find(&no_id, &ElementName::P);
    assert_eq!(style_of(&styled_by_class, p).text_align, TextAlign::End);

    // Equal specificity: the later rule wins — across sheets too.
    let plain = parse(br#"<html><body><p>text</p></body></html>"#).unwrap();
    let styled_late = styled(
        &plain,
        &["p { text-align: start }", "p { text-align: center }"],
    );
    let p = find(&plain, &ElementName::P);
    assert_eq!(style_of(&styled_late, p).text_align, TextAlign::Center);
}

#[test]
fn inline_style_beats_publisher() {
    let doc = parse(
        br#"<html><body><p id="x" style="text-align: end">text</p></body></html>"#,
    )
    .unwrap();
    let styled = styled(&doc, &["#x { text-align: center !important }"]);
    let p = find(&doc, &ElementName::P);
    assert_eq!(style_of(&styled, p).text_align, TextAlign::End);
}

#[test]
fn display_none_hides_subtree() {
    let doc = parse(
        br#"<html><body>
<div class="hidden"><p>gone <em>all</em> gone</p></div>
<p>kept</p>
</body></html>"#,
    )
    .unwrap();
    let styled = styled(&doc, &[".hidden { display: none }"]);

    let div = find(&doc, &ElementName::Div);
    // Every node of the subtree, text included, is display_none.
    let mut work = vec![div];
    let mut seen = 0;
    while let Some(id) = work.pop() {
        assert!(style_of(&styled, id).display_none, "node {id:?} visible");
        seen += 1;
        work.extend(doc.node(id).children.iter().copied());
    }
    assert!(seen >= 4, "subtree unexpectedly small: {seen}");

    // The sibling paragraph is unaffected.
    let kept = (0..doc.nodes.len() as u32)
        .map(NodeId)
        .filter(|&id| doc.element(id).is_some_and(|d| d.name == ElementName::P))
        .nth(1)
        .unwrap();
    assert!(!style_of(&styled, kept).display_none);
}

#[test]
fn writing_mode_from_body_only() {
    let on_div = parse(br#"<html><body><div class="v">text</div></body></html>"#).unwrap();
    let styled_div = styled(&on_div, &[".v { writing-mode: vertical-rl }"]);
    assert_eq!(styled_div.writing_mode, WritingMode::HorizontalTb);

    let on_body = parse(br#"<html><body><p>text</p></body></html>"#).unwrap();
    let styled_body = styled(&on_body, &["body { writing-mode: vertical-rl }"]);
    assert_eq!(styled_body.writing_mode, WritingMode::VerticalRl);

    // Inline style on body works too — and the vertical ruby fixture's
    // own inline sheet drives the same path end to end.
    let inline = parse(
        br#"<html><body style="writing-mode: vertical-rl"><p>text</p></body></html>"#,
    )
    .unwrap();
    assert_eq!(styled(&inline, &[]).writing_mode, WritingMode::VerticalRl);

    let ruby = parse(crate::test_support::CJK_VERTICAL_RUBY_DOC.as_bytes()).unwrap();
    let css: Vec<String> = ruby
        .stylesheets
        .iter()
        .filter_map(|source| match source {
            StylesheetSource::Inline(css) => Some(css.clone()),
            StylesheetSource::Linked(_) => None,
        })
        .collect();
    let refs: Vec<&str> = css.iter().map(String::as_str).collect();
    assert_eq!(styled(&ruby, &refs).writing_mode, WritingMode::VerticalRl);
}

#[test]
fn unsupported_writing_mode_values_never_override_vertical() {
    // A legacy/vendor value (tb-rl, inherit, …) later in the cascade is
    // skipped entirely — it must not flip an honored vertical-rl back to
    // horizontal.
    let doc = parse("<html><body><p>縦書き</p></body></html>".as_bytes()).unwrap();
    let styled_legacy = styled(
        &doc,
        &["html { writing-mode: vertical-rl } body { writing-mode: tb-rl }"],
    );
    assert_eq!(styled_legacy.writing_mode, WritingMode::VerticalRl);

    let styled_inherit = styled(
        &doc,
        &["body { writing-mode: vertical-rl } body { writing-mode: inherit }"],
    );
    assert_eq!(styled_inherit.writing_mode, WritingMode::VerticalRl);

    // horizontal-tb itself stays honored… by being the default: with no
    // vertical-rl anywhere, the resource is horizontal.
    let styled_plain = styled(&doc, &["body { writing-mode: horizontal-tb }"]);
    assert_eq!(styled_plain.writing_mode, WritingMode::HorizontalTb);
}

#[test]
fn dir_attr_sets_direction() {
    let doc = parse(RTL_DOC.as_bytes()).unwrap();
    let styled = styled(&doc, &[]);

    let body = find(&doc, &ElementName::Body);
    let p = find(&doc, &ElementName::P);
    assert_eq!(style_of(&styled, body).direction, Direction::Rtl);
    // Direction inherits into the subtree.
    assert_eq!(style_of(&styled, p).direction, Direction::Rtl);
    // But the html element above the marked subtree stays ltr.
    assert_eq!(style_of(&styled, doc.root).direction, Direction::Ltr);
}

#[test]
fn unsupported_css_ignored() {
    let doc = parse(br#"<html><body><p>text</p></body></html>"#).unwrap();
    let styled = styled(
        &doc,
        &[r#"
@media screen { p { text-align: center } }
p::first-line { text-align: center }
p { float: left; color: red }
p + p { text-align: center }
"#],
    );
    let p = find(&doc, &ElementName::P);
    // Everything above was skipped; the UA default survives.
    assert_eq!(style_of(&styled, p).text_align, TextAlign::Justify);
    assert!(!style_of(&styled, p).display_none);
}

#[test]
fn cjk_doc_defaults() {
    let doc = parse(CJK_HORIZONTAL_DOC.as_bytes()).unwrap();
    let styled = styled(&doc, &[]);

    assert_eq!(styled.writing_mode, WritingMode::HorizontalTb);
    let p = find(&doc, &ElementName::P);
    let style = style_of(&styled, p);
    assert_eq!(style.text_align, TextAlign::Justify);
    assert_eq!(style.direction, Direction::Ltr);
    assert_eq!(style.font_weight, FontWeight::Normal);
    // The h1 is UA-bold.
    let h1 = find(&doc, &ElementName::H1);
    assert_eq!(style_of(&styled, h1).font_weight, FontWeight::Bold);
}
