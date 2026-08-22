//! EPUB test fixture builders, shared with dependents' test suites via
//! the `test-support` feature. Every test builds its own books in a
//! tempdir — no binary fixtures live in git.

use std::io::Write;
use std::path::Path;

#[derive(Clone, Copy, PartialEq)]
pub enum TocKind {
    Nav,
    Ncx,
    None,
}

/// What the fixture's manifest declares for cover art.
#[derive(Clone, Copy, PartialEq)]
pub enum CoverKind {
    /// No cover item at all.
    None,
    /// A well-formed `image/png` cover.
    Png,
    /// An unrecognized media type paired with an href whose last dot is
    /// followed by a path separator, so the extension derived from the
    /// href is a path fragment rather than a suffix.
    MalformedHref,
}

/// Builds a valid EPUB exercising the full import pipeline: two CJK
/// spine chapters, a nested TOC (nav doc or NCX), and a cover image.
pub fn write_epub_with(
    path: &Path,
    title: &str,
    author: &str,
    language: &str,
    toc: TocKind,
    cover: CoverKind,
) {
    let file = std::fs::File::create(path).unwrap();
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
    match cover {
        CoverKind::None => {}
        CoverKind::Png => manifest.push_str(
            r#"<item id="cover-img" href="images/cover.png" media-type="image/png" properties="cover-image"/>"#,
        ),
        CoverKind::MalformedHref => manifest.push_str(
            r#"<item id="cover-img" href="img.old/cover" media-type="image/x-unknown" properties="cover-image"/>"#,
        ),
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
<body><h1 id="s1">第一章</h1><p>月の光が窓辺に落ちていた。</p></body></html>"#
            .as_bytes(),
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

    match cover {
        CoverKind::None => {}
        CoverKind::Png => {
            zip.start_file("OEBPS/images/cover.png", stored).unwrap();
            zip.write_all(b"\x89PNG\r\n\x1a\nfake png bytes").unwrap();
        }
        CoverKind::MalformedHref => {
            zip.start_file("OEBPS/img.old/cover", stored).unwrap();
            zip.write_all(b"\x89PNG\r\n\x1a\nfake png bytes").unwrap();
        }
    }
    zip.finish().unwrap();
}

/// Builds an EPUB from a caller-supplied OPF plus extra archive entries,
/// for fixtures whose manifest/spine/TOC shape the stock builder cannot
/// express (the parser-cap tests). Writes `mimetype` and a `container.xml`
/// pointing at `OEBPS/content.opf`; each extra entry is an
/// (`OEBPS/`-relative name, content) pair, deflated so oversized fixtures
/// stay small on disk.
pub fn write_epub_parts(path: &Path, opf: &str, entries: &[(&str, &str)]) {
    let file = std::fs::File::create(path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let deflated = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

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

    zip.start_file("OEBPS/content.opf", deflated).unwrap();
    zip.write_all(opf.as_bytes()).unwrap();

    for (name, content) in entries {
        zip.start_file(format!("OEBPS/{name}"), deflated).unwrap();
        zip.write_all(content.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
}

/// The default fixture: nav TOC + cover.
pub fn write_epub(path: &Path, title: &str, author: &str, language: &str) {
    write_epub_with(path, title, author, language, TocKind::Nav, CoverKind::Png);
}

/// General-purpose fixture builder for the reader-engine suites: caller
/// declares resources, spine order, TOC entries, and package flags, and
/// gets a valid EPUB 3 on disk. XHTML resources are passed as full
/// documents. Test-only code: I/O errors panic.
pub struct EpubBuilder {
    language: String,
    resources: Vec<(String, String, Vec<u8>)>,
    spine: Vec<String>,
    toc: Vec<(String, String, u32)>,
    rtl: bool,
    pre_paginated: bool,
}

impl Default for EpubBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl EpubBuilder {
    pub fn new() -> Self {
        Self {
            language: "en".to_string(),
            resources: Vec::new(),
            spine: Vec::new(),
            toc: Vec::new(),
            rtl: false,
            pre_paginated: false,
        }
    }

    /// Sets `dc:language` (a BCP 47 tag).
    pub fn language(mut self, tag: &str) -> Self {
        self.language = tag.to_string();
        self
    }

    /// Adds a manifest resource at `href` (package-root-relative, stored
    /// under `OEBPS/`) with the given media type and bytes.
    pub fn resource(mut self, href: &str, media_type: &str, bytes: &[u8]) -> Self {
        self.resources
            .push((href.to_string(), media_type.to_string(), bytes.to_vec()));
        self
    }

    /// Declares the spine itemrefs, in order. Every href must also be
    /// declared via [`EpubBuilder::resource`].
    pub fn spine(mut self, hrefs: &[&str]) -> Self {
        self.spine = hrefs.iter().map(|h| h.to_string()).collect();
        self
    }

    /// Declares TOC entries as `(title, href, depth)`; a non-empty list
    /// emits an EPUB 3 nav doc. Depths deeper than parent+1 are clamped
    /// by the nav's nesting.
    pub fn toc(mut self, entries: &[(&str, &str, u32)]) -> Self {
        self.toc = entries
            .iter()
            .map(|(t, h, d)| (t.to_string(), h.to_string(), *d))
            .collect();
        self
    }

    /// Sets `page-progression-direction="rtl"` on the spine.
    pub fn rtl_progression(mut self) -> Self {
        self.rtl = true;
        self
    }

    /// Declares `rendition:layout` as `pre-paginated`.
    pub fn pre_paginated(mut self) -> Self {
        self.pre_paginated = true;
        self
    }

    /// Writes the EPUB to `path`. Panics on I/O errors (test-only code).
    pub fn write(self, path: &Path) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let stored = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        let deflated = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

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

        let mut manifest = String::new();
        for (at, (href, media_type, _)) in self.resources.iter().enumerate() {
            manifest.push_str(&format!(
                r#"<item id="r{at}" href="{}" media-type="{}"/>"#,
                xml_escape(href),
                xml_escape(media_type)
            ));
        }
        if !self.toc.is_empty() {
            manifest.push_str(
                r#"<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>"#,
            );
        }

        let mut itemrefs = String::new();
        for href in &self.spine {
            let at = self
                .resources
                .iter()
                .position(|(h, _, _)| h == href)
                .unwrap_or_else(|| panic!("spine href {href} was never declared as a resource"));
            itemrefs.push_str(&format!(r#"<itemref idref="r{at}"/>"#));
        }
        let progression = if self.rtl {
            r#" page-progression-direction="rtl""#
        } else {
            ""
        };
        let rendition = if self.pre_paginated {
            r#"<meta property="rendition:layout">pre-paginated</meta>"#
        } else {
            ""
        };

        zip.start_file("OEBPS/content.opf", deflated).unwrap();
        zip.write_all(
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="uid">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="uid">fixture</dc:identifier>
    <dc:title>Fixture</dc:title>
    <dc:creator>Fixture Author</dc:creator>
    <dc:language>{}</dc:language>
    {rendition}
  </metadata>
  <manifest>{manifest}</manifest>
  <spine{progression}>{itemrefs}</spine>
</package>"#,
                xml_escape(&self.language)
            )
            .as_bytes(),
        )
        .unwrap();

        for (href, _, bytes) in &self.resources {
            zip.start_file(format!("OEBPS/{href}"), deflated).unwrap();
            zip.write_all(bytes).unwrap();
        }

        if !self.toc.is_empty() {
            zip.start_file("OEBPS/nav.xhtml", deflated).unwrap();
            zip.write_all(nav_doc(&self.toc).as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }
}

/// Renders `(title, href, depth)` entries as a nested EPUB 3 nav doc.
fn nav_doc(entries: &[(String, String, u32)]) -> String {
    let mut out = String::from(
        r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body>
<nav epub:type="toc"><ol>"#,
    );
    // Depth of the currently open <li> chain; each nesting level adds an
    // <ol> inside the parent's still-open <li>.
    let mut depth = 0u32;
    let mut first = true;
    for (title, href, entry_depth) in entries {
        let entry_depth = if first { 0 } else { (*entry_depth).min(depth + 1) };
        if !first {
            if entry_depth > depth {
                out.push_str("<ol>");
            } else {
                out.push_str("</li>");
                for _ in entry_depth..depth {
                    out.push_str("</ol></li>");
                }
            }
        }
        out.push_str(&format!(
            r#"<li><a href="{}">{}</a>"#,
            xml_escape(href),
            xml_escape(title)
        ));
        depth = entry_depth;
        first = false;
    }
    if !first {
        out.push_str("</li>");
        for _ in 0..depth {
            out.push_str("</ol></li>");
        }
    }
    out.push_str("</ol></nav></body></html>");
    out
}

/// Minimal XML escaping for fixture text and attribute values.
fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
