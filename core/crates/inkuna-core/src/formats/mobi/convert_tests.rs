use std::io::Read;
use std::path::Path;

use super::{coalesce, convert_to_epub, Chunk, ImageBudget};
use crate::formats::epub;
use crate::test_support::MobiTestBuilder;
use crate::CoreError;

fn chapter(path: &Path, index: usize) -> String {
    let file = std::fs::File::open(path).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    let mut body = String::new();
    archive
        .by_name(&format!("OEBPS/text/ch{index:05}.xhtml"))
        .unwrap()
        .read_to_string(&mut body)
        .unwrap();
    body
}

fn filepos_fixture() -> (Vec<u8>, usize, usize) {
    let template = concat!(
        r#"<a filepos="0000000000">same</a><h1>One</h1><p>first</p>"#,
        r#"<a filepos=0000000000>next</a><mbp:pagebreak/><h1>Two</h1><p>second</p>"#,
    );
    let first = template.find("<h1>One").unwrap();
    let second = template.find("<h1>Two").unwrap();
    let mut html = template.to_string();
    html = html.replacen("0000000000", &format!("{first:010}"), 1);
    html = html.replacen("0000000000", &format!("{second:010}"), 1);
    (html.into_bytes(), first, second)
}

#[test]
fn splits_pagebreaks_and_rewrites_same_and_cross_filepos_links() {
    let dir = tempfile::tempdir().unwrap();
    let mobi = dir.path().join("links.mobi");
    let epub_path = dir.path().join("links.epub");
    let (html, first, second) = filepos_fixture();
    MobiTestBuilder::new(6).html(&html).write(&mobi);

    convert_to_epub(&mobi, &epub_path, "Fallback").unwrap();

    let package = epub::read_package(&epub_path).unwrap();
    assert_eq!(package.spine.len(), 2);
    assert_eq!(package.toc[0].title, "One");
    assert_eq!(package.toc[1].title, "Two");
    let first_chapter = chapter(&epub_path, 1);
    let second_chapter = chapter(&epub_path, 2);
    assert!(first_chapter.contains(&format!(r##"href="#fp{first}""##)));
    assert!(first_chapter.contains(&format!(r#"id="fp{first}""#)));
    assert!(first_chapter.contains(&format!(r#"href="ch00002.xhtml#fp{second}""#)));
    assert!(second_chapter.contains(&format!(r#"id="fp{second}""#)));
    assert!(!first_chapter.contains("filepos"));
    assert!(!second_chapter.contains("mbp:"));
}

#[test]
fn consecutive_pagebreaks_do_not_shift_cross_filepos_chapter_hrefs() {
    let dir = tempfile::tempdir().unwrap();
    let mobi = dir.path().join("empty-split.mobi");
    let epub_path = dir.path().join("empty-split.epub");
    let template = concat!(
        r#"<a filepos="0000000000">last</a><p>first</p>"#,
        "<mbp:pagebreak/><mbp:pagebreak/><h1>Last</h1><p>body</p>",
    );
    let target = template.find("<h1>Last").unwrap();
    let html = template.replacen("0000000000", &format!("{target:010}"), 1);
    MobiTestBuilder::new(6).html(html.as_bytes()).write(&mobi);

    convert_to_epub(&mobi, &epub_path, "Fallback").unwrap();

    let package = epub::read_package(&epub_path).unwrap();
    assert_eq!(package.spine.len(), 2);
    assert!(chapter(&epub_path, 1).contains(&format!(r#"href="ch00002.xhtml#fp{target}""#)));
    assert!(chapter(&epub_path, 2).contains(&format!(r#"id="fp{target}""#)));
}

#[test]
fn extracts_recindex_images_and_cover_while_dropping_missing_images() {
    let dir = tempfile::tempdir().unwrap();
    let mobi = dir.path().join("images.mobi");
    let epub_path = dir.path().join("images.epub");
    let png = b"\x89PNG\r\n\x1a\nimage";
    MobiTestBuilder::new(6)
        .html(b"<h1>Pictures</h1><img recindex='1'/><img recindex='2'/>")
        .exth(201, &0u32.to_be_bytes())
        .image(png)
        .write(&mobi);

    convert_to_epub(&mobi, &epub_path, "Fallback").unwrap();

    let body = chapter(&epub_path, 1);
    assert!(body.contains(r#"src="../images/img00001.png" alt="""#));
    assert!(!body.contains("img00002"));
    let package = epub::read_package(&epub_path).unwrap();
    assert_eq!(package.cover.unwrap().bytes, png);
    let mut archive = zip::ZipArchive::new(std::fs::File::open(&epub_path).unwrap()).unwrap();
    assert!(archive.by_name("OEBPS/images/img00001.png").is_ok());
}

#[test]
fn decodes_cp1252_and_preserves_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let mobi = dir.path().join("cp1252.mobi");
    let epub_path = dir.path().join("cp1252.epub");
    MobiTestBuilder::new(6)
        .encoding(1252)
        .fullname(b"Caf\xe9")
        .exth(100, b"Andr\xe9")
        .html(b"<h1>Entr\xe9e</h1><p>It\x92s ready.</p>")
        .write(&mobi);

    convert_to_epub(&mobi, &epub_path, "Fallback").unwrap();

    let package = epub::read_package(&epub_path).unwrap();
    assert_eq!(package.metadata.title.as_deref(), Some("Café"));
    assert_eq!(package.metadata.authors, ["André"]);
    assert_eq!(package.metadata.language.as_deref(), Some("en"));
    assert_eq!(package.toc[0].title, "Entrée");
    let text = epub::extract_spine_text(&epub_path, &package.spine_hrefs());
    assert!(text[0].as_deref().unwrap().contains("It’s ready."));
}

#[test]
fn round_trips_cjk_and_uses_zh_fallback_chapter_titles() {
    let dir = tempfile::tempdir().unwrap();
    let mobi = dir.path().join("中文.mobi");
    let epub_path = dir.path().join("中文.epub");
    MobiTestBuilder::new(6)
        .name(b"")
        .locale(0x0804)
        .html("<p>松风入夜。</p><mbp:pagebreak/><p>月照长街。</p>".as_bytes())
        .write(&mobi);

    convert_to_epub(&mobi, &epub_path, "月光書房").unwrap();

    let package = epub::read_package(&epub_path).unwrap();
    assert_eq!(package.metadata.title.as_deref(), Some("月光書房"));
    assert_eq!(package.metadata.language.as_deref(), Some("zh"));
    assert_eq!(
        package
            .toc
            .iter()
            .map(|entry| entry.title.as_str())
            .collect::<Vec<_>>(),
        ["第1章", "第2章"]
    );
    let text = epub::extract_spine_text(&epub_path, &package.spine_hrefs());
    assert!(text[0].as_deref().unwrap().contains("松风入夜。"));
    assert!(text[1].as_deref().unwrap().contains("月照长街。"));
}

#[test]
fn splits_a_large_stream_without_pagebreaks_near_block_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    let mobi = dir.path().join("large.mobi");
    let epub_path = dir.path().join("large.epub");
    let paragraph = format!("<p>{}</p>", "bounded text ".repeat(2_000));
    let html = paragraph.repeat(20);
    let records = html
        .as_bytes()
        .chunks(400 * 1024)
        .map(<[u8]>::to_vec)
        .collect();
    MobiTestBuilder::new(6).text_records(records).write(&mobi);

    convert_to_epub(&mobi, &epub_path, "Large").unwrap();

    let package = epub::read_package(&epub_path).unwrap();
    assert!((2..=3).contains(&package.spine.len()));
    for index in 1..=package.spine.len() {
        assert!(chapter(&epub_path, index).len() <= 1024 * 1024);
    }
}

#[test]
fn fallback_utf8_encoding_never_splits_a_cjk_codepoint() {
    let dir = tempfile::tempdir().unwrap();
    let mobi = dir.path().join("fallback-utf8.mobi");
    let epub_path = dir.path().join("fallback-utf8.epub");
    let text = "月".repeat(100_000);
    MobiTestBuilder::new(6)
        .encoding(0)
        .html(text.as_bytes())
        .write(&mobi);

    convert_to_epub(&mobi, &epub_path, "Fallback").unwrap();

    let package = epub::read_package(&epub_path).unwrap();
    let extracted = epub::extract_spine_text(&epub_path, &package.spine_hrefs())
        .into_iter()
        .flatten()
        .fold(String::new(), |mut all, text| {
            all.push_str(&text);
            all
        });
    assert_eq!(extracted, text);
}

#[test]
fn oversized_tag_is_rejected_without_panicking_at_a_chunk_boundary() {
    let dir = tempfile::tempdir().unwrap();
    let mobi = dir.path().join("oversized-tag.mobi");
    let mut html = String::from("<a filepos=\"1\" data-padding=\"");
    html.push_str(&"1".repeat(1_200 * 1024));
    html.push_str("\">body</a>");
    let records = html
        .as_bytes()
        .chunks(400 * 1024)
        .map(<[u8]>::to_vec)
        .collect();
    MobiTestBuilder::new(6).text_records(records).write(&mobi);

    let result = std::panic::catch_unwind(|| {
        convert_to_epub(&mobi, &dir.path().join("oversized-tag.epub"), "Tag")
    });

    assert!(result.is_ok(), "oversized tag panicked");
    let result = result.unwrap();
    assert!(
        matches!(result, Err(CoreError::InvalidPublication(_))),
        "unexpected result: {result:?}"
    );
}

#[test]
fn accepted_long_tag_stays_whole_across_the_target_chunk_boundary() {
    let dir = tempfile::tempdir().unwrap();
    let mobi = dir.path().join("long-tag.mobi");
    let epub_path = dir.path().join("long-tag.epub");
    let mut html = String::from("<a filepos=\"1\" data-padding=\"");
    html.push_str(&"1".repeat(400 * 1024));
    html.push_str("\">body</a>");
    let records = html
        .as_bytes()
        .chunks(400 * 1024)
        .map(<[u8]>::to_vec)
        .collect();
    MobiTestBuilder::new(6).text_records(records).write(&mobi);

    convert_to_epub(&mobi, &epub_path, "Tag").unwrap();

    let package = epub::read_package(&epub_path).unwrap();
    let text = epub::extract_spine_text(&epub_path, &package.spine_hrefs());
    assert!(text.iter().flatten().any(|body| body.contains("body")));
}

#[test]
fn pathological_coalescing_keeps_raw_chapters_below_the_expansion_budget() {
    let mut chunks = Vec::with_capacity(10_001);
    let mut start = 0;
    for length in std::iter::repeat_n(700 * 1024, 2).chain(std::iter::repeat_n(1, 9_999)) {
        chunks.push(Chunk {
            start,
            end: start + length,
        });
        start += length;
    }

    coalesce(&mut chunks);

    assert!(chunks.len() <= 9_990);
    assert!(chunks
        .iter()
        .all(|chunk| chunk.end - chunk.start <= 1024 * 1024));
}

#[test]
fn image_budget_rejects_an_asset_that_would_exceed_the_aggregate_cap() {
    let mut budget = ImageBudget::new(10);
    assert!(budget.reserve(6));
    assert!(!budget.reserve(5));
    assert!(budget.reserve(4));
    assert!(!budget.reserve(1));
}

#[test]
fn converts_pure_kf8_and_prefers_the_kf8_half_of_a_combo() {
    let dir = tempfile::tempdir().unwrap();
    let pure = dir.path().join("pure.azw3");
    MobiTestBuilder::new(8)
        .kf8_files(vec![crate::test_support::Kf8FileFixture::new(
            b"<body><p>new only</p></body>",
        )])
        .write(&pure);
    let pure_epub = dir.path().join("pure.epub");
    convert_to_epub(&pure, &pure_epub, "Pure").unwrap();
    let pure_package = epub::read_package(&pure_epub).unwrap();
    assert!(epub::extract_spine_text(&pure_epub, &pure_package.spine_hrefs())[0]
        .as_deref()
        .unwrap()
        .contains("new only"));

    let combo = dir.path().join("combo.mobi");
    let mut kf8 = MobiTestBuilder::new(8);
    kf8.kf8_files(vec![crate::test_support::Kf8FileFixture::new(
        b"<body><p>new markup</p></body>",
    )]);
    MobiTestBuilder::new(6)
        .html(b"<p>old markup</p>")
        .kf8(kf8)
        .write(&combo);
    let converted = dir.path().join("combo.epub");
    convert_to_epub(&combo, &converted, "Combo").unwrap();
    let package = epub::read_package(&converted).unwrap();
    let text = epub::extract_spine_text(&converted, &package.spine_hrefs());
    assert!(text[0].as_deref().unwrap().contains("new markup"));
    assert!(!text[0].as_deref().unwrap().contains("old markup"));
}
