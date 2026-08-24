use super::{
    cap_sheet_sources, parse_sheet, resolve, Direction, FontStyle, FontWeight, StyledDocument,
    TextAlign, WritingMode,
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
    assert_eq!(style_of(&styled, outside).font_weight, FontWeight::BOLD);
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
fn cap_sheet_sources_drops_whole_sheets_from_end() {
    let sheets = ["aaaa", "bbb", "cc"];

    // The budget cuts between sheets, never inside one.
    assert_eq!(cap_sheet_sources(&sheets, 9), ["aaaa", "bbb", "cc"]);
    assert_eq!(cap_sheet_sources(&sheets, 8), ["aaaa", "bbb"]);
    assert_eq!(cap_sheet_sources(&sheets, 7), ["aaaa", "bbb"]);
    assert_eq!(cap_sheet_sources(&sheets, 6), ["aaaa"]);
    assert_eq!(cap_sheet_sources(&sheets, 4), ["aaaa"]);
    // Even the first sheet drops when it alone exceeds the budget.
    assert!(cap_sheet_sources(&sheets, 3).is_empty());
    assert!(cap_sheet_sources(&[], 100).is_empty());
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
    assert_eq!(style.font_weight, FontWeight::NORMAL);
    // The h1 is UA-bold.
    let h1 = find(&doc, &ElementName::H1);
    assert_eq!(style_of(&styled, h1).font_weight, FontWeight::BOLD);
}

#[test]
fn numeric_font_weights_cascade() {
    let doc = parse(
        br#"<html><body>
<p class="thin">thin</p>
<p class="semi">semi</p>
<p class="frac">frac</p>
<p class="bad">bad</p>
</body></html>"#,
    )
    .unwrap();
    let styled = styled(
        &doc,
        &[".thin { font-weight: 250 } .semi { font-weight: 600 } \
           .frac { font-weight: 450.4 } .bad { font-weight: 1200 }"],
    );
    let weight_of = |class: &str| {
        let id = (0..doc.nodes.len() as u32)
            .map(NodeId)
            .find(|&id| {
                doc.element(id)
                    .is_some_and(|data| data.class.as_deref() == Some(class))
            })
            .unwrap();
        style_of(&styled, id).font_weight
    };
    assert_eq!(weight_of("thin"), FontWeight::new(250));
    assert_eq!(weight_of("semi"), FontWeight::new(600));
    // Fractional weights are legal CSS and round to the nearest integer.
    assert_eq!(weight_of("frac"), FontWeight::new(450));
    // Out-of-range numbers are invalid and drop the declaration.
    assert_eq!(weight_of("bad"), FontWeight::NORMAL);
}

#[test]
fn bolder_and_lighter_resolve_against_the_inherited_weight() {
    let doc = parse(
        br#"<html><body>
<div class="light"><p><span class="up">x</span></p></div>
<div class="heavy"><p><span class="down">x</span></p></div>
</body></html>"#,
    )
    .unwrap();
    let styled = styled(
        &doc,
        &[".light { font-weight: 300 } .heavy { font-weight: 900 } \
           .up { font-weight: bolder } .down { font-weight: lighter }"],
    );
    let weight_of = |class: &str| {
        let id = (0..doc.nodes.len() as u32)
            .map(NodeId)
            .find(|&id| {
                doc.element(id)
                    .is_some_and(|data| data.class.as_deref() == Some(class))
            })
            .unwrap();
        style_of(&styled, id).font_weight
    };
    // css-fonts-4 relative table: bolder(300) = 400, lighter(900) = 700.
    assert_eq!(weight_of("up"), FontWeight::new(400));
    assert_eq!(weight_of("down"), FontWeight::new(700));
}

#[test]
fn font_weight_relative_table() {
    // The css-fonts-4 relative-weight mapping, spot-checked across all
    // its bands.
    let cases = [
        (100, 400, 100),
        (300, 400, 100),
        (400, 700, 100),
        (500, 700, 100),
        (600, 900, 400),
        (700, 900, 400),
        (800, 900, 700),
        (900, 900, 700),
    ];
    for (inherited, bolder, lighter) in cases {
        let w = FontWeight::new(inherited);
        assert_eq!(w.bolder(), FontWeight::new(bolder), "bolder({inherited})");
        assert_eq!(w.lighter(), FontWeight::new(lighter), "lighter({inherited})");
    }
    // Clamping and the bold threshold.
    assert_eq!(FontWeight::new(0), FontWeight::new(1));
    assert_eq!(FontWeight::new(2000), FontWeight::new(1000));
    assert!(!FontWeight::new(599).is_bold());
    assert!(FontWeight::new(600).is_bold());
}

// --- font-family + @font-face (package B2) ---------------------------

use super::{FamilyName, FontFaceRule};

fn stack_of<'s>(styled: &'s StyledDocument<'_>, id: NodeId) -> &'s [FamilyName] {
    styled.family_stack(style_of(styled, id).font_family)
}

#[test]
fn font_family_stacks_parse_inherit_and_intern() {
    let doc = parse(
        br#"<html><body>
<div class="serif"><p>inherits <em style="font-family: 'Custom Face', sans-serif">inline</em></p></div>
</body></html>"#,
    )
    .unwrap();
    let styled = styled(
        &doc,
        &[".serif { font-family: Crimson Text, serif }"],
    );

    // The rule's stack: unquoted multi-ident name + generic keyword.
    let p = find(&doc, &ElementName::P);
    assert_eq!(
        stack_of(&styled, p),
        &[
            FamilyName::Named("Crimson Text".to_string()),
            FamilyName::Serif
        ]
    );
    // The div's stack inherited into p, so both share one interned id.
    let div = find(&doc, &ElementName::Div);
    assert_eq!(
        style_of(&styled, div).font_family,
        style_of(&styled, p).font_family
    );

    // The inline style overrides with a quoted name + generic.
    let em = find(&doc, &ElementName::Em);
    assert_eq!(
        stack_of(&styled, em),
        &[
            FamilyName::Named("Custom Face".to_string()),
            FamilyName::SansSerif
        ]
    );

    // Table: empty stack at 0, plus the two distinct stacks.
    assert_eq!(styled.families.len(), 3);
    assert!(styled.family_stack(Default::default()).is_empty());
}

#[test]
fn font_family_generics_fold_case_insensitively() {
    let doc = parse(br#"<html><body><p>x</p></body></html>"#).unwrap();
    let styled = styled(&doc, &["p { font-family: SERIF, Sans-Serif, MONOSPACE }"]);
    let p = find(&doc, &ElementName::P);
    assert_eq!(
        stack_of(&styled, p),
        &[
            FamilyName::Serif,
            FamilyName::SansSerif,
            FamilyName::Monospace
        ]
    );
}

/// An invalid value drops the whole declaration, browser-style, and a
/// quoted generic stays a NAME (CSS: quoting removes keyword meaning).
#[test]
fn font_family_invalid_values_drop_and_quoted_generics_stay_named() {
    let doc = parse(br#"<html><body><p>x</p></body></html>"#).unwrap();
    let styled_bad = styled(&doc, &["p { font-family: 12px, serif }"]);
    let p = find(&doc, &ElementName::P);
    assert!(stack_of(&styled_bad, p).is_empty());

    let styled_quoted = styled(&doc, &[r#"p { font-family: "serif" }"#]);
    assert_eq!(
        stack_of(&styled_quoted, p),
        &[FamilyName::Named("serif".to_string())]
    );
}

#[test]
fn font_face_rules_parse_family_style_weight_and_sources() {
    let sheet = parse_sheet(
        r#"
@font-face {
  font-family: "Publisher Serif";
  font-style: italic;
  font-weight: 300 700;
  src: local("Skip Me"), url(../fonts/pub.woff2) format("woff2"),
       url("fonts/pub.ttf");
}
@font-face { font-family: Solo; src: url(solo.otf); }
@font-face { src: url(nameless.ttf); }
@font-face { font-family: NoSrc; }
p { font-style: italic }
"#,
    );
    assert_eq!(
        sheet.font_faces(),
        &[
            FontFaceRule {
                family: "Publisher Serif".to_string(),
                style: FontStyle::Italic,
                weight: (300, 700),
                sources: vec![
                    "../fonts/pub.woff2".to_string(),
                    "fonts/pub.ttf".to_string()
                ],
                unicode_ranges: None,
            },
            FontFaceRule {
                family: "Solo".to_string(),
                style: FontStyle::Normal,
                weight: (400, 400),
                sources: vec!["solo.otf".to_string()],
                unicode_ranges: None,
            },
        ],
        "rules without a family or without any url source drop"
    );
    // The qualified rule after the at-rules still parsed.
    assert_eq!(sheet.rules.len(), 1);
}

/// `@media`-wrapped rules stay skipped wholesale (including any
/// `@font-face` inside), and an unknown at-rule never poisons what
/// follows it.
#[test]
fn other_at_rules_still_skip_around_font_face() {
    let sheet = parse_sheet(
        r#"
@charset "utf-8";
@media print { @font-face { font-family: Hidden; src: url(h.ttf); } }
@font-face { font-family: Kept; src: url(k.ttf); }
"#,
    );
    assert_eq!(sheet.font_faces().len(), 1);
    assert_eq!(sheet.font_faces()[0].family, "Kept");
}

/// A single @font-face weight is a degenerate range; keywords map.
#[test]
fn font_face_weight_forms() {
    let sheet = parse_sheet(
        r#"@font-face { font-family: A; font-weight: bold; src: url(a.ttf); }
@font-face { font-family: B; font-weight: 250; src: url(b.ttf); }
@font-face { font-family: C; font-weight: 700 300; src: url(c.ttf); }"#,
    );
    let weights: Vec<(u16, u16)> = sheet.font_faces().iter().map(|r| r.weight).collect();
    assert_eq!(weights, vec![(700, 700), (250, 250), (300, 700)]);
}

/// C3: `unicode-range` descriptors parse into sorted, merged inclusive
/// ranges — single codepoints, ranges, `?` wildcards, comma lists,
/// case-insensitively; a malformed descriptor is ignored whole (the
/// face then claims every codepoint, CSS's invalid-descriptor rule).
#[test]
fn font_face_unicode_range_parses_and_merges() {
    let sheet = parse_sheet(
        r#"@font-face {
            font-family: Subset;
            src: url(subset.woff2);
            unicode-range: u+61, U+41-5A, U+30??, U+55-60;
        }"#,
    );
    assert_eq!(
        sheet.font_faces()[0].unicode_ranges,
        // 41-5A and 55-60 overlap and merge; 61 is adjacent and merges too.
        Some(vec![(0x41, 0x61), (0x3000, 0x30FF)])
    );

    let absent = parse_sheet(r#"@font-face { font-family: A; src: url(a.ttf); }"#);
    assert_eq!(absent.font_faces()[0].unicode_ranges, None);

    for bad in [
        "unicode-range: U+GGGG;",
        "unicode-range: U+110000;",
        "unicode-range: U+5A-41;",
        "unicode-range: U+1?2?;",
        "unicode-range: 41-5A;",
    ] {
        let css = format!(
            "@font-face {{ font-family: B; src: url(b.ttf); {bad} }}"
        );
        let sheet = parse_sheet(&css);
        assert_eq!(
            sheet.font_faces()[0].unicode_ranges,
            None,
            "malformed descriptor {bad:?} must be ignored whole"
        );
    }
}
