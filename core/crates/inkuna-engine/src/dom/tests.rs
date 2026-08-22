use super::{
    parse, Document, ElementName, NodeId, NodeKind, StylesheetSource, MAX_DEPTH, MAX_DOM_NODES,
    MAX_STYLESHEET_BYTES, MAX_TEXT_BYTES,
};
use crate::test_support::{CJK_HORIZONTAL_DOC, MALFORMED_DOC};
use crate::EngineError;

/// All element nodes with the given name, in document order.
fn find_all(doc: &Document, name: &ElementName) -> Vec<NodeId> {
    (0..doc.nodes.len() as u32)
        .map(NodeId)
        .filter(|&id| doc.element(id).is_some_and(|data| data.name == *name))
        .collect()
}

/// The concatenated text of a subtree, markup seams flattened.
fn subtree_text(doc: &Document, id: NodeId) -> String {
    let mut out = String::new();
    let mut stack = vec![id];
    while let Some(id) = stack.pop() {
        let node = doc.node(id);
        if let NodeKind::Text(text) = &node.kind {
            out.push_str(text);
        }
        for &child in node.children.iter().rev() {
            stack.push(child);
        }
    }
    out
}

#[test]
fn parses_well_formed_cjk_doc() {
    let doc = parse(CJK_HORIZONTAL_DOC.as_bytes()).unwrap();
    assert!(!doc.truncated);

    let paragraphs = find_all(&doc, &ElementName::P);
    assert_eq!(paragraphs.len(), 2);
    assert_eq!(
        subtree_text(&doc, paragraphs[0]),
        "月光洒在窗台上，屋里一片寂静。"
    );
    assert_eq!(
        subtree_text(&doc, paragraphs[1]),
        "他放下手中的书，望向远处的群山。"
    );
    let headings = find_all(&doc, &ElementName::H1);
    assert_eq!(headings.len(), 1);
    assert_eq!(subtree_text(&doc, headings[0]), "第一章");
    // The <title> lives in head and must produce no arena nodes.
    assert!(!subtree_text(&doc, doc.root).contains("中文"));
}

#[test]
fn recovers_unclosed_inline() {
    let doc = parse(MALFORMED_DOC.as_bytes()).unwrap();
    let emphases = find_all(&doc, &ElementName::Em);
    assert_eq!(emphases.len(), 1);
    assert_eq!(subtree_text(&doc, emphases[0]), "unclosed emphasis runs on.");
    // The stray & survives as literal text.
    assert!(subtree_text(&doc, doc.root).contains("Fish & chips."));
}

#[test]
fn skips_script_scans_head() {
    let doc = parse(
        br#"<html><head>
<title>HeadTitle</title>
<link rel="STYLESHEET" type="text/css" href="css/main.css"/>
<style>p { color: red; }</style>
</head>
<body><p>kept</p><script>var dropped = 1;</script></body></html>"#,
    )
    .unwrap();

    assert_eq!(
        doc.stylesheets,
        vec![
            StylesheetSource::Linked("css/main.css".to_string()),
            StylesheetSource::Inline("p { color: red; }".to_string()),
        ]
    );
    let text = subtree_text(&doc, doc.root);
    assert!(text.contains("kept"));
    assert!(!text.contains("dropped"));
    assert!(!text.contains("HeadTitle"), "head title text leaked: {text}");
}

#[test]
fn anchor_map_collects_ids() {
    let doc = parse(
        br#"<html><body>
<div id="outer"><p id="first">a</p><p>b</p><span id="inner">c</span></div>
<p id="last">d</p>
</body></html>"#,
    )
    .unwrap();

    let ids: Vec<&str> = doc.anchors.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(ids, vec!["outer", "first", "inner", "last"]);
    for (id, node) in &doc.anchors {
        assert_eq!(doc.element(*node).unwrap().id.as_deref(), Some(id.as_str()));
    }
}

#[test]
fn node_budget_truncates_not_errors() {
    let mut xhtml = String::from("<html><body>");
    for _ in 0..300_000 {
        xhtml.push_str("<p>x</p>");
    }
    xhtml.push_str("</body></html>");

    let doc = parse(xhtml.as_bytes()).unwrap();
    assert!(doc.truncated);
    assert!(doc.nodes.len() <= MAX_DOM_NODES);
    // The prefix is intact and renderable.
    assert!(!find_all(&doc, &ElementName::P).is_empty());
}

#[test]
fn html_void_tags_in_head_never_swallow_body() {
    // <meta> and <link> written HTML-style, without `/>`: the head must
    // still end at </head> and the body must parse as body.
    let doc = parse(
        br#"<html><head><meta charset="utf-8"><link rel="stylesheet" href="a.css"><title>T</title></head><body><p>body text</p></body></html>"#,
    )
    .unwrap();

    assert!(!doc.truncated);
    let text = subtree_text(&doc, doc.root);
    assert!(text.contains("body text"), "body swallowed by head: {text:?}");
    assert!(!text.contains('T'), "head title leaked: {text:?}");
    // The HTML-style <link> is still scanned as a stylesheet source.
    assert_eq!(
        doc.stylesheets,
        vec![StylesheetSource::Linked("a.css".to_string())]
    );
    assert_eq!(find_all(&doc, &ElementName::P).len(), 1);
}

#[test]
fn unclosed_style_recovers_at_head_end() {
    // </head> while <style> is still open: the captured CSS is kept and
    // the body parses normally.
    let doc = parse(
        br#"<html><head><style>p { color: red }</head><body><p>after</p></body></html>"#,
    )
    .unwrap();

    assert_eq!(
        doc.stylesheets,
        vec![StylesheetSource::Inline("p { color: red }".to_string())]
    );
    assert!(subtree_text(&doc, doc.root).contains("after"));
    assert_eq!(find_all(&doc, &ElementName::P).len(), 1);
}

#[test]
fn unclosed_style_recovers_at_body_start() {
    // Neither </style> nor </head>: <body> itself is the recovery point.
    let doc =
        parse(br#"<html><head><style>em { font-style: normal }<body><p>kept</p></body></html>"#)
            .unwrap();

    assert_eq!(
        doc.stylesheets,
        vec![StylesheetSource::Inline("em { font-style: normal }".to_string())]
    );
    assert!(subtree_text(&doc, doc.root).contains("kept"));
}

#[test]
fn unclosed_style_flushes_at_eof() {
    let doc = parse(br#"<html><head><style>p { color: red }"#).unwrap();
    assert_eq!(
        doc.stylesheets,
        vec![StylesheetSource::Inline("p { color: red }".to_string())]
    );
}

#[test]
fn depth_budget_flattens_and_relocates_ids() {
    const NESTED: usize = MAX_DEPTH + 40;
    let mut xhtml = String::from("<html><body>");
    for _ in 0..NESTED {
        xhtml.push_str("<div>");
    }
    xhtml.push_str(r#"<span id="deep">bottom</span>"#);
    for _ in 0..NESTED {
        xhtml.push_str("</div>");
    }
    xhtml.push_str("</body></html>");

    let doc = parse(xhtml.as_bytes()).unwrap();
    // Flattening is silent recovery, not truncation.
    assert!(!doc.truncated);
    // Elements at MAX_DEPTH and beyond produced no arena nodes…
    assert!(doc.nodes.len() <= MAX_DEPTH + 2);
    // …but the flattened text still attaches to the retained ancestor.
    assert!(subtree_text(&doc, doc.root).contains("bottom"));
    assert!(find_all(&doc, &ElementName::Span).is_empty());

    // The flattened span's id relocated onto that ancestor (a div), so
    // the projection's anchor map still resolves it.
    let (id, node) = doc
        .anchors
        .iter()
        .find(|(id, _)| id == "deep")
        .expect("flattened id lost");
    assert_eq!(id, "deep");
    assert_eq!(doc.element(*node).unwrap().name, ElementName::Div);
    assert!(subtree_text(&doc, *node).contains("bottom"));
}

#[test]
fn text_budget_truncates_not_errors() {
    let body = "a".repeat(MAX_TEXT_BYTES + 16);
    let xhtml = format!("<html><body><p>{body}</p><p>never reached</p></body></html>");

    let doc = parse(xhtml.as_bytes()).unwrap();
    assert!(doc.truncated);
    let total: usize = doc
        .nodes
        .iter()
        .filter_map(|node| match &node.kind {
            NodeKind::Text(text) => Some(text.len()),
            NodeKind::Element(_) => None,
        })
        .sum();
    assert!(total <= MAX_TEXT_BYTES);
    // The kept prefix is intact.
    assert!(subtree_text(&doc, doc.root).starts_with("aaaa"));
}

#[test]
fn stylesheet_budget_truncates_css_keeps_body() {
    let css = "x".repeat(MAX_STYLESHEET_BYTES + 64);
    let xhtml =
        format!("<html><head><style>{css}</style></head><body><p>still here</p></body></html>");

    let doc = parse(xhtml.as_bytes()).unwrap();
    let [StylesheetSource::Inline(kept)] = doc.stylesheets.as_slice() else {
        panic!("expected one inline sheet, got {:?}", doc.stylesheets.len());
    };
    assert_eq!(kept.len(), MAX_STYLESHEET_BYTES);
    // A capped stylesheet never costs the chapter its text.
    assert!(subtree_text(&doc, doc.root).contains("still here"));
}

#[test]
fn garbage_input_fails_closed() {
    let garbage: Vec<u8> = (0..4096u32).map(|i| (i % 251) as u8 | 0x80).collect();
    match parse(&garbage) {
        Err(EngineError::UnsupportedContent { .. }) => {}
        other => panic!("expected UnsupportedContent, got {other:?}"),
    }
}

#[test]
fn attr_budget_truncates_on_char_boundary() {
    let class = "漢".repeat(3000); // 9000 bytes of 3-byte chars
    let xhtml = format!(r#"<html><body><p class="{class}">x</p></body></html>"#);
    let doc = parse(xhtml.as_bytes()).unwrap();

    let p = find_all(&doc, &ElementName::P)[0];
    let kept = doc.element(p).unwrap().class.as_deref().unwrap();
    assert!(kept.len() <= super::MAX_ATTR_BYTES);
    assert!(kept.chars().all(|c| c == '漢'));
    // 4096 is not a multiple of 3: the cut backed up to a boundary.
    assert_eq!(kept.len(), 4095);
}
