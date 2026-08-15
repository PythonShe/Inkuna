//! Shared test fixtures. Every test builds its own books in a tempdir —
//! no binary fixtures live in git.

use std::io::Write;
use std::path::Path;

use crate::{ImportOutcome, Publication};

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum TocKind {
    Nav,
    Ncx,
    None,
}

/// Builds a valid EPUB exercising the full import pipeline: two CJK
/// spine chapters, a nested TOC (nav doc or NCX), and a cover image.
pub(crate) fn write_epub_with(
    path: &Path,
    title: &str,
    author: &str,
    language: &str,
    toc: TocKind,
    cover: bool,
) {
    let file = std::fs::File::create(path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let stored = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored);

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

    let mut manifest = String::from(
        r#"<item id="c1" href="text/ch01.xhtml" media-type="application/xhtml+xml"/>
    <item id="c2" href="text/ch02.xhtml" media-type="application/xhtml+xml"/>"#,
    );
    let mut spine_attr = String::new();
    match toc {
        TocKind::Nav => manifest.push_str(
            r#"<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>"#,
        ),
        TocKind::Ncx => {
            manifest.push_str(
                r#"<item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>"#,
            );
            spine_attr = r#" toc="ncx""#.to_string();
        }
        TocKind::None => {}
    }
    if cover {
        manifest.push_str(
            r#"<item id="cover-img" href="images/cover.png" media-type="image/png" properties="cover-image"/>"#,
        );
    }

    zip.start_file("OEBPS/content.opf", stored).unwrap();
    zip.write_all(
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="uid">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="uid">test</dc:identifier>
    <dc:title>{title}</dc:title>
    <dc:creator>{author}</dc:creator>
    <dc:language>{language}</dc:language>
  </metadata>
  <manifest>{manifest}</manifest>
  <spine{spine_attr}><itemref idref="c1"/><itemref idref="c2"/></spine>
</package>"#
        )
        .as_bytes(),
    )
    .unwrap();

    zip.start_file("OEBPS/text/ch01.xhtml", stored).unwrap();
    zip.write_all(
        r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>ch01</title></head>
<body><h1 id="s1">第一章</h1><p>月の光が窓辺に落ちていた。</p></body></html>"#.as_bytes(),
    )
    .unwrap();
    zip.start_file("OEBPS/text/ch02.xhtml", stored).unwrap();
    zip.write_all(
        br#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>ch02</title></head>
<body><p>Second chapter text.</p></body></html>"#,
    )
    .unwrap();

    match toc {
        TocKind::Nav => {
            zip.start_file("OEBPS/nav.xhtml", stored).unwrap();
            zip.write_all(
                r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body>
<nav epub:type="toc"><ol>
  <li><a href="text/ch01.xhtml">第一章</a>
    <ol><li><a href="text/ch01.xhtml#s1">第一節</a></li></ol>
  </li>
  <li><a href="text/ch02.xhtml">第二章</a></li>
</ol></nav></body></html>"#.as_bytes(),
            )
            .unwrap();
        }
        TocKind::Ncx => {
            zip.start_file("OEBPS/toc.ncx", stored).unwrap();
            zip.write_all(
                r#"<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/"><navMap>
<navPoint id="a"><navLabel><text>第一章</text></navLabel><content src="text/ch01.xhtml"/>
  <navPoint id="b"><navLabel><text>第一節</text></navLabel><content src="text/ch01.xhtml#s1"/></navPoint>
</navPoint>
<navPoint id="c"><navLabel><text>第二章</text></navLabel><content src="text/ch02.xhtml"/></navPoint>
</navMap></ncx>"#.as_bytes(),
            )
            .unwrap();
        }
        TocKind::None => {}
    }

    if cover {
        zip.start_file("OEBPS/images/cover.png", stored).unwrap();
        zip.write_all(b"\x89PNG\r\n\x1a\nfake png bytes").unwrap();
    }
    zip.finish().unwrap();
}

/// The default fixture: nav TOC + cover.
pub(crate) fn write_epub(path: &Path, title: &str, author: &str, language: &str) {
    write_epub_with(path, title, author, language, TocKind::Nav, true);
}

pub(crate) fn write_cbz(path: &Path) {
    let file = std::fs::File::create(path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let stored = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored);
    zip.start_file("001.jpg", stored).unwrap();
    zip.write_all(&[0xFF, 0xD8, 0xFF]).unwrap();
    zip.finish().unwrap();
}

/// Builds a minimal PalmDB "BOOKMOBI" file whose MOBI header carries the
/// given file version (6 = classic MOBI, 8 = KF8/AZW3).
pub(crate) fn write_mobi(path: &Path, version: u32) {
    let mut bytes = vec![0u8; 78];
    bytes[60..68].copy_from_slice(b"BOOKMOBI");
    bytes[76..78].copy_from_slice(&1u16.to_be_bytes());
    let record0_offset = 78 + 8;
    bytes.extend_from_slice(&(record0_offset as u32).to_be_bytes());
    bytes.extend_from_slice(&[0u8; 4]);
    let mut record0 = [0u8; 40];
    record0[16..20].copy_from_slice(b"MOBI");
    record0[36..40].copy_from_slice(&version.to_be_bytes());
    bytes.extend_from_slice(&record0);
    std::fs::write(path, bytes).unwrap();
}

pub(crate) fn imported(outcome: ImportOutcome) -> Publication {
    match outcome {
        ImportOutcome::Imported(p) => p,
        ImportOutcome::Duplicate(p) => panic!("expected fresh import, got duplicate of {}", p.id),
    }
}
