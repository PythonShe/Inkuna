use super::*;

#[test]
fn extracts_normalized_text_with_cjk() {
    let xml = r#"<html><head><title>skip me</title><style>p{}</style></head>
<body><h1>第一章　月光</h1>
<p>静かな　夜だった。</p><p>Line <em>two</em> here.</p>
<script>ignore();</script>
<p>&amp; escaped &lt;text&gt;</p></body></html>"#;
    let text = extract_text(xml).unwrap();
    // U+3000 ideographic spaces normalize to ASCII spaces with the rest.
    assert_eq!(
        text,
        "第一章 月光\n静かな 夜だった。\nLine two here.\n& escaped <text>"
    );
}
