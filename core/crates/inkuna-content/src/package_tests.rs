use crate::test_support::EpubBuilder;
use crate::{read_package, RenditionLayout, TocEntry};

const CH1: &str = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>1</title></head>
<body><h1 id="s1">第一章</h1><p>本文。</p></body></html>"#;
const CH2: &str = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>2</title></head>
<body><p>Second chapter.</p></body></html>"#;

#[test]
fn epub_builder_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("built.epub");
    EpubBuilder::new()
        .language("ja")
        .resource("text/ch01.xhtml", "application/xhtml+xml", CH1.as_bytes())
        .resource("text/ch02.xhtml", "application/xhtml+xml", CH2.as_bytes())
        .spine(&["text/ch02.xhtml", "text/ch01.xhtml"])
        .toc(&[
            ("第一章", "text/ch01.xhtml", 0),
            ("第一節", "text/ch01.xhtml#s1", 1),
            ("第二章", "text/ch02.xhtml", 0),
        ])
        .rtl_progression()
        .pre_paginated()
        .write(&path);

    let package = read_package(&path).unwrap();
    assert_eq!(package.metadata.language.as_deref(), Some("ja"));
    assert_eq!(
        package.spine_hrefs(),
        vec!["OEBPS/text/ch02.xhtml", "OEBPS/text/ch01.xhtml"]
    );
    assert!(package.page_progression_rtl);
    assert_eq!(package.rendition_layout, RenditionLayout::PrePaginated);
    assert_eq!(
        package.toc,
        vec![
            TocEntry {
                title: "第一章".to_string(),
                href: "OEBPS/text/ch01.xhtml".to_string(),
                depth: 0,
            },
            TocEntry {
                title: "第一節".to_string(),
                href: "OEBPS/text/ch01.xhtml#s1".to_string(),
                depth: 1,
            },
            TocEntry {
                title: "第二章".to_string(),
                href: "OEBPS/text/ch02.xhtml".to_string(),
                depth: 0,
            },
        ]
    );
}
