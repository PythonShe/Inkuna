use std::fs::File;
use std::io::Read;

use super::*;
use crate::formats::epub::{extract_spine_text, read_package};
use crate::test_support::imported;
use crate::{CoreError, Library};

const COVER_BYTES: &[u8] = b"\x89PNG\r\n\x1a\nwriter cover";

fn invalid_message(error: CoreError) -> String {
    match error {
        CoreError::InvalidPublication(message) => message,
        other => panic!("expected invalid publication, got {other}"),
    }
}

fn basic_writer(title: &str) -> EpubWriter {
    let mut writer = EpubWriter::new(title);
    writer.author("紫式部");
    writer.language("ja");
    writer.add_chapter("第一章", "<h1>第一章</h1><p>月の光が窓辺に落ちていた。</p>");
    writer
}

#[test]
fn writer_round_trips_cjk_metadata_nested_toc_text_and_cover() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("round-trip.epub");
    let mut writer = EpubWriter::new("月光書房");
    writer.author("紫式部");
    writer.author("清少納言");
    writer.language("ja");
    writer.set_cover(COVER_BYTES.to_vec(), "image/png");
    writer.add_chapter("序章", "<h1>序章</h1><p>静かな夜。</p>");
    writer.begin_volume("上巻");
    writer.add_chapter("第一章", "<h1>第一章</h1><p>月の光が窓辺に落ちていた。</p>");
    writer.add_chapter("第二章", "<h1>第二章</h1><p>風が竹林を渡った。</p>");
    writer.finish(&path).unwrap();

    let package = read_package(&path).unwrap();
    assert_eq!(package.metadata.title.as_deref(), Some("月光書房"));
    assert_eq!(package.metadata.authors, ["紫式部", "清少納言"]);
    assert_eq!(package.metadata.language.as_deref(), Some("ja"));
    assert_eq!(package.spine.len(), 3);
    assert_eq!(
        package.toc,
        [
            crate::formats::epub::TocEntry {
                title: "序章".into(),
                href: "OEBPS/text/ch00001.xhtml".into(),
                depth: 0,
            },
            crate::formats::epub::TocEntry {
                title: "上巻".into(),
                href: "OEBPS/text/ch00002.xhtml".into(),
                depth: 0,
            },
            crate::formats::epub::TocEntry {
                title: "第一章".into(),
                href: "OEBPS/text/ch00002.xhtml".into(),
                depth: 1,
            },
            crate::formats::epub::TocEntry {
                title: "第二章".into(),
                href: "OEBPS/text/ch00003.xhtml".into(),
                depth: 1,
            },
        ]
    );
    let texts = extract_spine_text(&path, &package.spine);
    assert_eq!(texts.len(), 3);
    assert!(texts[1]
        .as_deref()
        .unwrap()
        .contains("月の光が窓辺に落ちていた。"));
    assert!(texts[2].as_deref().unwrap().contains("風が竹林を渡った。"));
    let cover = package.cover.unwrap();
    assert_eq!(cover.bytes, COVER_BYTES);
    assert_eq!(cover.extension, "png");
}

#[test]
fn writer_emits_empty_volume_as_a_spine_title_page() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty-volume.epub");
    let mut writer = EpubWriter::new("全集");
    writer.begin_volume("空巻");
    writer.finish(&path).unwrap();

    let package = read_package(&path).unwrap();
    assert_eq!(package.metadata.language.as_deref(), Some("und"));
    assert_eq!(package.spine, ["OEBPS/text/ch00001.xhtml"]);
    assert_eq!(
        package.toc,
        [crate::formats::epub::TocEntry {
            title: "空巻".into(),
            href: "OEBPS/text/ch00001.xhtml".into(),
            depth: 0,
        }]
    );
    let texts = extract_spine_text(&path, &package.spine);
    assert!(texts[0].as_deref().unwrap().contains("空巻"));
}

#[test]
fn writer_xml_escapes_titles_and_all_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("escaped.epub");
    let raw = "<月&光>\"'";
    let mut writer = EpubWriter::new(raw);
    writer.author(raw);
    writer.language("x-<&>\"'");
    writer.begin_volume(raw);
    writer.add_chapter(raw, "<p>trusted &amp; pre-escaped</p>");
    writer.finish(&path).unwrap();

    let package = read_package(&path).unwrap();
    assert_eq!(package.metadata.title.as_deref(), Some(raw));
    assert_eq!(package.metadata.authors, [raw]);
    assert_eq!(package.metadata.language.as_deref(), Some("x-<&>\"'"));
    assert_eq!(package.toc[0].title, raw);
    assert_eq!(package.toc[1].title, raw);

    let file = File::open(&path).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    for name in [
        "OEBPS/content.opf",
        "OEBPS/nav.xhtml",
        "OEBPS/text/ch00001.xhtml",
    ] {
        let mut entry = archive.by_name(name).unwrap();
        let mut xml = String::new();
        entry.read_to_string(&mut xml).unwrap();
        assert!(
            xml.contains("&lt;月&amp;光&gt;&quot;&apos;"),
            "{name}: {xml}"
        );
        assert!(
            !xml.contains(raw),
            "raw XML-sensitive value leaked into {name}"
        );
    }
}

#[test]
fn writer_emits_required_epub3_archive_layout_and_resources() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("layout.epub");
    let mut writer = basic_writer("Archive Layout");
    writer.stylesheet("body { color: rebeccapurple; }");
    let image_href = writer.add_image("illustration.png", b"image bytes".to_vec(), "image/png");
    assert_eq!(image_href, "../images/illustration.png");
    writer.finish(&path).unwrap();

    let file = File::open(&path).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    let names: Vec<String> = archive.file_names().map(str::to_owned).collect();
    assert_eq!(names[0], "mimetype");
    assert!(names.contains(&"META-INF/container.xml".into()));
    assert!(names.contains(&"OEBPS/content.opf".into()));
    assert!(names.contains(&"OEBPS/nav.xhtml".into()));
    assert!(names.contains(&"OEBPS/style.css".into()));
    assert!(names.contains(&"OEBPS/text/ch00001.xhtml".into()));
    assert!(names.contains(&"OEBPS/images/illustration.png".into()));
    assert!(!names.iter().any(|name| name.ends_with(".ncx")));

    for index in 0..archive.len() {
        let entry = archive.by_index(index).unwrap();
        let expected = if index == 0 {
            zip::CompressionMethod::Stored
        } else {
            zip::CompressionMethod::Deflated
        };
        assert_eq!(entry.compression(), expected, "{}", entry.name());
        assert_eq!(entry.last_modified(), Some(zip::DateTime::default()));
    }

    let mut mimetype = String::new();
    archive
        .by_name("mimetype")
        .unwrap()
        .read_to_string(&mut mimetype)
        .unwrap();
    assert_eq!(mimetype, "application/epub+zip");
    let mut css = String::new();
    archive
        .by_name("OEBPS/style.css")
        .unwrap()
        .read_to_string(&mut css)
        .unwrap();
    assert!(css.contains("p { text-indent: 2em; margin: 0.2em 0; }"));
    assert!(css.contains("hr.scene { border: none; text-align: center; margin: 1.5em 0; }"));
    assert!(css.contains("h1, h2, h3, h4, h5, h6 { text-align: center; }"));
    assert!(css.contains("body { color: rebeccapurple; }"));

    let mut opf = String::new();
    archive
        .by_name("OEBPS/content.opf")
        .unwrap()
        .read_to_string(&mut opf)
        .unwrap();
    assert!(opf.contains("<meta property=\"dcterms:modified\">1970-01-01T00:00:00Z</meta>"));
    let identifier = opf
        .split_once("urn:uuid:")
        .unwrap()
        .1
        .split_once('<')
        .unwrap()
        .0;
    assert_eq!(
        uuid::Uuid::parse_str(identifier).unwrap().get_version_num(),
        4
    );
}

#[test]
fn writer_rejects_more_than_the_spine_item_cap() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("spine-overflow.epub");
    let mut writer = EpubWriter::new("Too Many Chapters");
    for _ in 0..=super::MAX_SPINE_ITEMS {
        writer.add_chapter("Chapter", "<p>x</p>");
    }

    let message = invalid_message(writer.finish(&path).unwrap_err());
    assert!(message.contains("spine"), "{message}");
    assert!(
        message.contains(&super::MAX_SPINE_ITEMS.to_string()),
        "{message}"
    );
}

#[test]
fn writer_rejects_more_than_the_toc_entry_cap() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("toc-overflow.epub");
    let mut writer = EpubWriter::new("Too Many Volumes");
    for _ in 0..=super::MAX_TOC_ENTRIES {
        writer.begin_volume("Volume");
    }

    let message = invalid_message(writer.finish(&path).unwrap_err());
    assert!(message.contains("TOC"), "{message}");
    assert!(
        message.contains(&super::MAX_TOC_ENTRIES.to_string()),
        "{message}"
    );
}

#[test]
fn writer_rejects_a_chapter_body_over_the_spine_entry_cap() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("chapter-overflow.epub");
    let mut writer = EpubWriter::new("Oversized Chapter");
    writer.add_chapter(
        "Chapter",
        &"x".repeat(super::MAX_SPINE_ENTRY_BYTES as usize + 1),
    );

    let message = invalid_message(writer.finish(&path).unwrap_err());
    assert!(message.contains("chapter body"), "{message}");
    assert!(
        message.contains(&super::MAX_SPINE_ENTRY_BYTES.to_string()),
        "{message}"
    );
}

#[test]
fn writer_rejects_a_spine_resource_when_the_wrapper_pushes_it_over_the_cap() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wrapped-chapter-overflow.epub");
    let mut writer = EpubWriter::new("Wrapped Chapter");
    writer.add_chapter(
        "Chapter",
        &"x".repeat(super::MAX_SPINE_ENTRY_BYTES as usize),
    );

    let message = invalid_message(writer.finish(&path).unwrap_err());
    assert!(message.contains("spine resource"), "{message}");
    assert!(message.contains(&super::MAX_SPINE_ENTRY_BYTES.to_string()));
}

#[test]
fn writer_rejects_an_empty_spine() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty.epub");

    let message = invalid_message(EpubWriter::new("Empty").finish(&path).unwrap_err());
    assert!(message.contains("at least one spine item"), "{message}");
}

#[test]
fn writer_is_byte_deterministic() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first.epub");
    let second = dir.path().join("second.epub");
    basic_writer("Deterministic").finish(&first).unwrap();
    basic_writer("Deterministic").finish(&second).unwrap();

    assert_eq!(
        std::fs::read(first).unwrap(),
        std::fs::read(second).unwrap()
    );
}

#[test]
fn written_epub_imports_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("importable.epub");
    basic_writer("書き出した本").finish(&path).unwrap();

    let library = Library::open(dir.path().join("library")).unwrap();
    let publication = imported(library.import(path.to_str().unwrap()).unwrap());
    assert_eq!(publication.title, "書き出した本");
    assert_eq!(publication.authors, ["紫式部"]);
    assert_eq!(publication.language.as_deref(), Some("ja"));
    assert_eq!(library.chapters(&publication.id).unwrap().len(), 1);
}
