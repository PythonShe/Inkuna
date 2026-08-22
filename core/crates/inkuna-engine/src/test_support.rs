//! Shared fixtures for the engine suites: the canonical XHTML documents
//! (CJK fixtures are mandatory — horizontal, vertical+ruby, RTL, and mixed
//! script all live here) and thin wrappers over inkuna-content's
//! `EpubBuilder`. Test-only code: I/O errors panic.
//!
//! The suites of movements 2–5 consume these incrementally; helpers a
//! landed movement has not reached yet are expected, not dead weight.
#![allow(dead_code)]

use std::path::PathBuf;

use inkuna_content::test_support::EpubBuilder;
use tempfile::TempDir;

/// Writes the built EPUB into `dir` and returns its path.
pub fn build_epub(dir: &TempDir, builder: EpubBuilder) -> PathBuf {
    let path = dir.path().join("fixture.epub");
    builder.write(&path);
    path
}

/// Latin prose: headings, paragraphs, inline emphasis, and a link whose
/// fragment target exists in the same document.
pub const LATIN_DOC: &str = r##"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Latin</title></head>
<body>
<h1 id="top">The Voyage</h1>
<p>The ship left <em>quietly</em> at dawn, its crew <strong>resolute</strong>.</p>
<h2 id="landfall">Landfall</h2>
<p>They sighted land on the <a href="#top">ninth</a> day.</p>
</body></html>"##;

/// Horizontal Chinese prose.
pub const CJK_HORIZONTAL_DOC: &str = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>中文</title></head>
<body>
<h1 id="chapter-one">第一章</h1>
<p>月光洒在窗台上，屋里一片寂静。</p>
<p>他放下手中的书，望向远处的群山。</p>
</body></html>"#;

/// Japanese vertical writing with ruby annotations: `vertical-rl` on the
/// body via an inline stylesheet, ruby bases with `rt` readings.
pub const CJK_VERTICAL_RUBY_DOC: &str = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>縦書き</title>
<style>body { writing-mode: vertical-rl; }</style></head>
<body>
<p><ruby>東京<rt>とうきょう</rt></ruby>の<ruby>空<rt>そら</rt></ruby>は高かった。</p>
</body></html>"#;

/// Hebrew text with `dir="rtl"`.
pub const RTL_DOC: &str = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>עברית</title></head>
<body dir="rtl">
<p>בראשית ברא אלהים את השמים ואת הארץ.</p>
</body></html>"#;

/// Latin, CJK, and digits mixed inside one paragraph.
pub const MIXED_SCRIPT_DOC: &str = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Mixed</title></head>
<body>
<p>The 東京 line opened in 1927 and carries 2000000 riders.</p>
</body></html>"#;

/// An image with explicit dimensions; pair with [`tiny_png`] bytes.
pub const IMAGE_DOC: &str = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Image</title></head>
<body>
<p>Before the plate.</p>
<img src="plate.png" width="120" height="80" alt="plate"/>
<p>After the plate.</p>
</body></html>"#;

/// A 2×2 table.
pub const TABLE_DOC: &str = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Table</title></head>
<body>
<table><caption>Totals</caption>
<tr><th>Year</th><th>Count</th></tr>
<tr><td>1901</td><td>417</td></tr>
</table>
</body></html>"#;

/// Malformed markup the parser must recover from: an unclosed `<em>` and
/// a stray `&`.
pub const MALFORMED_DOC: &str = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Broken</title></head>
<body>
<p>An <em>unclosed emphasis runs on.</p>
<p>Fish & chips.</p>
</body></html>"#;

/// A minimal valid PNG carrying the given dimensions: signature, IHDR,
/// and IEND — enough for any header-reading dimension probe. Generated at
/// runtime so no binary fixture lives in git.
pub fn tiny_png(w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(45);
    out.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    // 8-bit greyscale, no interlace.
    let mut ihdr = Vec::with_capacity(17);
    ihdr.extend_from_slice(b"IHDR");
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 0, 0, 0, 0]);
    push_chunk(&mut out, &ihdr);
    push_chunk(&mut out, b"IEND");
    out
}

/// Appends one PNG chunk (length, type+data, CRC).
fn push_chunk(out: &mut Vec<u8>, type_and_data: &[u8]) {
    out.extend_from_slice(&((type_and_data.len() - 4) as u32).to_be_bytes());
    out.extend_from_slice(type_and_data);
    out.extend_from_slice(&crc32(type_and_data).to_be_bytes());
}

/// Plain bitwise CRC-32 (ISO 3309), as PNG requires. Test-only, so
/// throughput is irrelevant.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// The one-XHTML-chapter builder most suites start from.
pub fn single_chapter(doc: &str) -> EpubBuilder {
    EpubBuilder::new()
        .resource("ch01.xhtml", "application/xhtml+xml", doc.as_bytes())
        .spine(&["ch01.xhtml"])
}

/// Convenience: builds a one-chapter EPUB in `dir` and returns its path.
pub fn single_chapter_epub(dir: &TempDir, doc: &str) -> PathBuf {
    build_epub(dir, single_chapter(doc))
}
