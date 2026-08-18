use std::io::Read;
use std::path::Path;

use super::{parse_position, sanitize_css};
use crate::formats::epub;
use crate::formats::mobi::convert::convert_to_epub;
use crate::test_support::{Kf8FileFixture, Kf8NcxFixture, MobiTestBuilder};

fn chapter(path: &Path, index: usize) -> String {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut value = String::new();
    archive
        .by_name(&format!("OEBPS/text/ch{index:05}.xhtml"))
        .unwrap()
        .read_to_string(&mut value)
        .unwrap();
    value
}

fn stylesheet(path: &Path) -> String {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut value = String::new();
    archive
        .by_name("OEBPS/style.css")
        .unwrap()
        .read_to_string(&mut value)
        .unwrap();
    value
}

#[test]
fn rewrites_kindle_positions_embeds_and_css_and_uses_ncx_nesting() {
    let dir = tempfile::tempdir().unwrap();
    let azw3 = dir.path().join("links.azw3");
    let epub_path = dir.path().join("links.epub");
    let first = concat!(
        "<html><head><link rel='stylesheet' href='kindle:flow:0001'/></head><body>",
        "<h1>Ignored heading</h1><section aid='old'><p>A</p></section>",
        "<a href='kindle:pos:fid:0000:off:0000000000'>same</a>",
        "<a href='kindle:pos:fid:0001:off:0000000000'>cross</a>",
        "<img src='kindle:embed:0001?mime=image/png' alt='art'/>",
        "</body></html>",
    );
    let second = "<html><body><article><h2>Second heading</h2><p>X</p></article></body></html>";
    let first_insert = first.find("</p>").unwrap();
    let second_insert = second.find("</p>").unwrap();
    let png = b"\x89PNG\r\n\x1a\nasset";
    MobiTestBuilder::new(8)
        .fullname("月光集".as_bytes())
        .locale(0x0804)
        .image(png)
        .kf8_files(vec![
            Kf8FileFixture::new(first.as_bytes()).fragment(first_insert, "一".as_bytes()),
            Kf8FileFixture::new(second.as_bytes()).fragment(second_insert, "二".as_bytes()),
        ])
        .css_flow(
            b"@import url('https://tracker.example/a.css'); p { color: black; background: url(//tracker.example/x); }",
        )
        .ncx(vec![
            Kf8NcxFixture::new("第一卷", 0, 1),
            Kf8NcxFixture::new("第二章", 1, 2),
        ])
        .write(&azw3);

    convert_to_epub(&azw3, &epub_path, "Fallback").unwrap();

    let package = epub::read_package(&epub_path).unwrap();
    assert_eq!(package.metadata.title.as_deref(), Some("月光集"));
    assert_eq!(package.spine.len(), 2);
    assert_eq!(
        package
            .toc
            .iter()
            .map(|entry| (entry.title.as_str(), entry.depth))
            .collect::<Vec<_>>(),
        [("第一卷", 0), ("第一卷", 1), ("第二章", 1)]
    );
    let first_chapter = chapter(&epub_path, 1);
    let second_chapter = chapter(&epub_path, 2);
    assert!(first_chapter.contains(r##"href="#kp0000""##));
    assert!(first_chapter.contains(r#"id="kp0000""#));
    assert!(first_chapter.contains(r#"href="ch00002.xhtml#kp0001""#));
    assert!(second_chapter.contains(r#"id="kp0001""#));
    assert!(first_chapter.contains(r#"src="../images/kf8img00001.png""#));
    assert!(!first_chapter.contains("kindle:"));
    assert!(!first_chapter.contains(" aid="));
    let css = stylesheet(&epub_path);
    assert!(css.contains("p { color: black;"));
    assert!(!css.contains("@import"));
    assert!(!css.contains("tracker.example"));
}

#[test]
fn missing_ncx_falls_back_to_headings_and_cjk_default_titles() {
    let dir = tempfile::tempdir().unwrap();
    let azw3 = dir.path().join("fallback.azw3");
    let epub_path = dir.path().join("fallback.epub");
    MobiTestBuilder::new(8)
        .locale(0x0804)
        .kf8_files(vec![
            Kf8FileFixture::new("<body><h1>序章</h1><p>松风。</p></body>".as_bytes()),
            Kf8FileFixture::new("<body><p>月影。</p></body>".as_bytes()),
        ])
        .write(&azw3);

    convert_to_epub(&azw3, &epub_path, "無題").unwrap();

    let package = epub::read_package(&epub_path).unwrap();
    assert_eq!(
        package
            .toc
            .iter()
            .map(|entry| entry.title.as_str())
            .collect::<Vec<_>>(),
        ["序章", "第2章"]
    );
    let text = epub::extract_spine_text(&epub_path, &package.spine);
    assert!(text[0].as_deref().unwrap().contains("松风。"));
    assert!(text[1].as_deref().unwrap().contains("月影。"));
}

#[test]
fn css_sanitizer_removes_imports_and_remote_urls_but_keeps_local_rules() {
    let css = sanitize_css(
        "@import \"evil.css\"; .a{background:url(https://x/a.png)} .b{background:url('../ok.png')}",
    );
    assert!(!css.contains("@import"));
    assert!(!css.contains("https://"));
    assert!(css.contains("../ok.png"));
}

#[test]
fn position_parser_accepts_the_full_ten_digit_base32_offset_space() {
    assert_eq!(
        parse_position("kindle:pos:fid:0001:off:VVVVVVVVVV"),
        Some(1)
    );
}
