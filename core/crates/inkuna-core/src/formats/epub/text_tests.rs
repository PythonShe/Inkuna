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

/// The aggregate corpus budget holds through the real extraction path: a
/// CJK resource whose text alone exceeds the (injected) budget comes back
/// `None` — dropped, import still succeeds upstream — even though it is
/// far under the per-entry decompression cap, while a resource within
/// the budget still extracts. Deterministic because each call names a
/// single resource, so rayon scheduling cannot reorder budget spending.
#[test]
fn aggregate_text_budget_drops_resources_beyond_it() {
    use std::io::Write;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("budget.epub");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("OEBPS/small.xhtml", stored).unwrap();
    zip.write_all("<html><body><p>短い本文。</p></body></html>".as_bytes())
        .unwrap();
    zip.start_file("OEBPS/big.xhtml", stored).unwrap();
    let big_body = "満月の夜、風が語る。".repeat(50); // ~1.5 KB of CJK text
    zip.write_all(format!("<html><body><p>{big_body}</p></body></html>").as_bytes())
        .unwrap();
    zip.finish().unwrap();

    let budget = 256;
    let small = extract_spine_text_budgeted(&path, &["OEBPS/small.xhtml".into()], budget);
    assert_eq!(small[0].as_deref(), Some("短い本文。"));

    let big = extract_spine_text_budgeted(&path, &["OEBPS/big.xhtml".into()], budget);
    assert!(big[0].is_none(), "over-budget resource must be dropped");
}

/// A spine may name the same resource any number of times. The parse must
/// stay bounded (spine cap), each distinct resource must be read and
/// extracted exactly once — the repeats sharing that one `Arc` rather than
/// re-reading the entry — and reading order must survive deduplication.
#[test]
fn repeated_spine_entries_are_extracted_once() {
    use std::io::Write;

    use super::super::opf::MAX_SPINE_ITEMS;
    use super::super::package::read_package;

    let repeats = MAX_SPINE_ITEMS + 2_000;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("repeated.epub");
    let file = std::fs::File::create(&path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    zip.start_file("mimetype", stored).unwrap();
    zip.write_all(b"application/epub+zip").unwrap();
    zip.start_file("META-INF/container.xml", stored).unwrap();
    zip.write_all(
        br#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#,
    )
    .unwrap();

    // ch02 leads, then ch01 repeats: order has to survive, and the repeats
    // must collapse to one extraction.
    let mut spine = String::from(r#"<itemref idref="c2"/>"#);
    for _ in 0..repeats {
        spine.push_str(r#"<itemref idref="c1"/>"#);
    }
    zip.start_file("OEBPS/content.opf", stored).unwrap();
    zip.write_all(
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="uid">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>反復</dc:title></metadata>
  <manifest>
    <item id="c1" href="text/ch01.xhtml" media-type="application/xhtml+xml"/>
    <item id="c2" href="text/ch02.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>{spine}</spine>
</package>"#
        )
        .as_bytes(),
    )
    .unwrap();
    zip.start_file("OEBPS/text/ch01.xhtml", stored).unwrap();
    zip.write_all(r#"<html><body><p>第一章の本文。</p></body></html>"#.as_bytes())
        .unwrap();
    zip.start_file("OEBPS/text/ch02.xhtml", stored).unwrap();
    zip.write_all(br#"<html><body><p>Second chapter text.</p></body></html>"#)
        .unwrap();
    zip.finish().unwrap();

    let package = read_package(&path).unwrap();
    assert_eq!(package.spine.len(), MAX_SPINE_ITEMS);

    let texts = extract_spine_text(&path, &package.spine);
    assert_eq!(texts.len(), MAX_SPINE_ITEMS);
    assert_eq!(texts[0].as_deref(), Some("Second chapter text."));

    let first_repeat = texts[1].clone().unwrap();
    assert_eq!(&*first_repeat, "第一章の本文。");
    for text in &texts[2..] {
        let repeat = text.clone().unwrap();
        // Same allocation, so the entry was read and extracted once.
        assert!(std::sync::Arc::ptr_eq(&first_repeat, &repeat));
    }
}
