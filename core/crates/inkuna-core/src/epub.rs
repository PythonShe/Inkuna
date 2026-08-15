//! EPUB parsing for import: OPF metadata, the spine (reading order), the
//! flattened TOC (EPUB 3 nav doc with NCX fallback), cover art, and
//! per-resource plain text — the future search corpus. Rendering and
//! pagination stay in the shells (Readium navigators); the core parses
//! once at import and never re-opens the book afterwards.
//!
//! Hrefs are stored package-root-relative, percent-decoded, without a
//! leading slash (e.g. `OEBPS/ch01.xhtml`), TOC hrefs keeping their
//! fragment. Chapter→resource mapping is `href`-minus-fragment matched
//! against the spine — derivable at query time, never stored.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use percent_encoding::percent_decode_str;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use rayon::prelude::*;

use crate::CoreError;

#[derive(Debug, Default)]
pub struct EpubMetadata {
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub language: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TocEntry {
    pub title: String,
    /// Package-root-relative, possibly carrying a `#fragment` — the
    /// Readium jump target.
    pub href: String,
    /// 0-based nesting depth in the flattened tree.
    pub depth: u32,
}

pub struct Cover {
    pub bytes: Vec<u8>,
    /// Lowercase filename extension derived from the media type.
    pub extension: String,
}

pub struct EpubPackage {
    pub metadata: EpubMetadata,
    /// Spine hrefs in reading order.
    pub spine: Vec<String>,
    pub toc: Vec<TocEntry>,
    pub cover: Option<Cover>,
}

/// Parses everything import needs in one pass over the archive: metadata,
/// spine, TOC, and cover bytes. Text extraction is separate
/// (`extract_spine_text`) so it can run in parallel.
pub fn read_package(path: &Path) -> Result<EpubPackage, CoreError> {
    let mut archive = zip::ZipArchive::new(File::open(path)?)?;

    let container = read_entry(&mut archive, "META-INF/container.xml")?;
    let opf_path = rootfile_path(&container)?;
    let opf_xml = read_entry(&mut archive, &opf_path)?;
    let opf_dir = parent_dir(&opf_path);
    let opf = parse_opf(&opf_xml);

    let resolve = |href: &str| resolve_href(opf_dir, href);
    let item_by_id = |id: &str| opf.items.iter().find(|i| i.id == id);

    let spine: Vec<String> = opf
        .spine_idrefs
        .iter()
        .filter_map(|idref| item_by_id(idref))
        .map(|item| resolve(&item.href))
        .collect();

    // EPUB 3 nav doc → NCX fallback. A nav doc that yields no entries
    // (present but empty or unparseable) also falls back.
    let mut toc = Vec::new();
    if let Some(nav_item) = opf.items.iter().find(|i| i.has_property("nav")) {
        let nav_path = resolve(&nav_item.href);
        if let Ok(xml) = read_entry(&mut archive, &nav_path) {
            toc = parse_nav(&xml, parent_dir(&nav_path));
        }
    }
    if toc.is_empty() {
        let ncx_item = opf
            .spine_toc
            .as_deref()
            .and_then(item_by_id)
            .or_else(|| {
                opf.items
                    .iter()
                    .find(|i| i.media_type == "application/x-dtbncx+xml")
            });
        if let Some(item) = ncx_item {
            let ncx_path = resolve(&item.href);
            if let Ok(xml) = read_entry(&mut archive, &ncx_path) {
                toc = parse_ncx(&xml, parent_dir(&ncx_path));
            }
        }
    }

    // Cover: EPUB 3 `cover-image` property → EPUB 2 `<meta name="cover">`
    // (whose content is the manifest id — or, in broken files, the href).
    let cover_item = opf
        .items
        .iter()
        .find(|i| i.has_property("cover-image"))
        .or_else(|| {
            opf.cover_meta.as_deref().and_then(|content| {
                item_by_id(content).or_else(|| opf.items.iter().find(|i| i.href == content))
            })
        });
    let cover = cover_item.and_then(|item| {
        let bytes = read_entry_bytes(&mut archive, &resolve(&item.href)).ok()?;
        Some(Cover {
            bytes,
            extension: image_extension(&item.media_type, &item.href),
        })
    });

    Ok(EpubPackage {
        metadata: opf.metadata,
        spine,
        toc,
        cover,
    })
}

/// Extracts plain text from every spine resource, in parallel across
/// resources. `None` marks a malformed or missing resource — the import
/// pipeline skips its text row (logged) and still succeeds.
pub fn extract_spine_text(path: &Path, spine: &[String]) -> Vec<Option<String>> {
    spine
        .par_iter()
        .map_init(
            // One archive handle per rayon worker split; zip readers need
            // &mut access, so they cannot be shared across threads.
            || zip::ZipArchive::new(File::open(path)?).map_err(CoreError::from),
            |archive, href| {
                let archive = archive.as_mut().ok()?;
                let xml = read_entry(archive, href).ok()?;
                let text = extract_text(&xml);
                if text.is_none() {
                    log::warn!("skipping text extraction for malformed resource {href}");
                }
                text
            },
        )
        .collect()
}

// ---------------------------------------------------------------------------
// Archive helpers

fn read_entry(archive: &mut zip::ZipArchive<File>, name: &str) -> Result<String, CoreError> {
    let mut entry = archive
        .by_name(name)
        .map_err(|_| CoreError::InvalidPublication(format!("missing {name}")))?;
    let mut buf = String::new();
    entry.read_to_string(&mut buf)?;
    Ok(buf)
}

fn read_entry_bytes(archive: &mut zip::ZipArchive<File>, name: &str) -> Result<Vec<u8>, CoreError> {
    let mut entry = archive
        .by_name(name)
        .map_err(|_| CoreError::InvalidPublication(format!("missing {name}")))?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf)?;
    Ok(buf)
}

// ---------------------------------------------------------------------------
// Path resolution

fn parent_dir(path: &str) -> &str {
    path.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("")
}

/// Resolves `href` (relative to `base_dir`, possibly percent-encoded,
/// possibly carrying a fragment) into a normalized package-root-relative
/// path, keeping the fragment verbatim.
fn resolve_href(base_dir: &str, href: &str) -> String {
    let (path, fragment) = match href.split_once('#') {
        Some((p, f)) => (p, Some(f)),
        None => (href, None),
    };
    let decoded = percent_decode_str(path).decode_utf8_lossy();

    let mut segments: Vec<&str> = if decoded.starts_with('/') {
        Vec::new()
    } else {
        base_dir.split('/').filter(|s| !s.is_empty()).collect()
    };
    for segment in decoded.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            s => segments.push(s),
        }
    }
    let mut resolved = segments.join("/");
    if let Some(fragment) = fragment {
        // A fragment-only href points into the referencing document itself;
        // `segments` already holds that document's path in that case.
        resolved.push('#');
        resolved.push_str(fragment);
    }
    resolved
}

fn image_extension(media_type: &str, href: &str) -> String {
    match media_type {
        "image/jpeg" => "jpg".into(),
        "image/png" => "png".into(),
        "image/gif" => "gif".into(),
        "image/svg+xml" => "svg".into(),
        "image/webp" => "webp".into(),
        _ => href
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_ascii_lowercase())
            .unwrap_or_else(|| "img".into()),
    }
}

// ---------------------------------------------------------------------------
// container.xml

fn rootfile_path(container_xml: &str) -> Result<String, CoreError> {
    let mut reader = Reader::from_str(container_xml);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if e.local_name().as_ref() == b"rootfile" => {
                if let Some(path) = attr_value(&e, b"full-path") {
                    return Ok(path);
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(CoreError::InvalidPublication(e.to_string())),
            _ => {}
        }
        buf.clear();
    }
    Err(CoreError::InvalidPublication("no rootfile in container.xml".into()))
}

// ---------------------------------------------------------------------------
// OPF

#[derive(Debug)]
struct ManifestItem {
    id: String,
    href: String,
    media_type: String,
    properties: String,
}

impl ManifestItem {
    fn has_property(&self, name: &str) -> bool {
        self.properties.split_ascii_whitespace().any(|p| p == name)
    }
}

#[derive(Debug, Default)]
struct Opf {
    metadata: EpubMetadata,
    items: Vec<ManifestItem>,
    spine_idrefs: Vec<String>,
    /// The spine's `toc` attribute (NCX manifest id), EPUB 2 style.
    spine_toc: Option<String>,
    /// `<meta name="cover" content="…">`, EPUB 2 style.
    cover_meta: Option<String>,
}

fn parse_opf(opf_xml: &str) -> Opf {
    let mut opf = Opf::default();
    let mut reader = Reader::from_str(opf_xml);
    let mut buf = Vec::new();
    // Tracks which dc: element we are inside so text (and entity-reference)
    // nodes accumulate into the right field, committed at the element's
    // end. Only the first title/language wins.
    let mut current: Option<&'static str> = None;
    let mut acc = String::new();
    loop {
        let event = reader.read_event_into(&mut buf);
        match &event {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let is_empty = matches!(&event, Ok(Event::Empty(_)));
                match e.local_name().as_ref() {
                    b"title" if !is_empty => current = Some("title"),
                    b"creator" if !is_empty => current = Some("creator"),
                    b"language" if !is_empty => current = Some("language"),
                    b"item" => {
                        opf.items.push(ManifestItem {
                            id: attr_value(e, b"id").unwrap_or_default(),
                            href: attr_value(e, b"href").unwrap_or_default(),
                            media_type: attr_value(e, b"media-type").unwrap_or_default(),
                            properties: attr_value(e, b"properties").unwrap_or_default(),
                        });
                    }
                    b"itemref" => {
                        if let Some(idref) = attr_value(e, b"idref") {
                            opf.spine_idrefs.push(idref);
                        }
                    }
                    b"spine" => opf.spine_toc = attr_value(e, b"toc"),
                    b"meta" => {
                        if attr_value(e, b"name").as_deref() == Some("cover") {
                            opf.cover_meta = attr_value(e, b"content");
                        }
                    }
                    _ if !is_empty => current = None,
                    _ => {}
                }
                if current.is_none() {
                    acc.clear();
                }
            }
            Ok(Event::Text(t)) => {
                if current.is_some() {
                    if let Ok(text) = t.decode() {
                        push_word(&mut acc, &text);
                    }
                }
            }
            Ok(Event::GeneralRef(r)) => {
                if current.is_some() {
                    acc.push_str(&resolve_ref(r));
                }
            }
            Ok(Event::End(_)) => {
                if let Some(field) = current.take() {
                    if let Some(text) = clean_text(Some(&acc)) {
                        match field {
                            "title" if opf.metadata.title.is_none() => {
                                opf.metadata.title = Some(text)
                            }
                            "creator" => opf.metadata.authors.push(text),
                            "language" if opf.metadata.language.is_none() => {
                                opf.metadata.language = Some(text)
                            }
                            _ => {}
                        }
                    }
                    acc.clear();
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    opf
}

// ---------------------------------------------------------------------------
// EPUB 3 nav doc

/// Parses the `<nav epub:type="toc">` (or `role="doc-toc"`) of a nav doc
/// into a flattened, depth-annotated list. Nesting depth follows `<ol>`
/// levels; entries are `<a>` elements with an href.
fn parse_nav(xml: &str, nav_dir: &str) -> Vec<TocEntry> {
    let mut entries = Vec::new();
    let mut reader = Reader::from_str(xml);
    reader.config_mut().check_end_names = false;
    let mut buf = Vec::new();

    let mut in_toc_nav = false;
    let mut nav_depth = 0u32; // <nav> nesting while inside the toc nav
    let mut ol_depth = 0u32;
    let mut link: Option<(String, String)> = None; // (href, title so far)

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match e.local_name().as_ref() {
                b"nav" if !in_toc_nav => {
                    let epub_type = raw_attr(&e, b"epub:type").unwrap_or_default();
                    let role = attr_value(&e, b"role").unwrap_or_default();
                    if epub_type.split_ascii_whitespace().any(|t| t == "toc")
                        || role.split_ascii_whitespace().any(|r| r == "doc-toc")
                    {
                        in_toc_nav = true;
                        nav_depth = 1;
                        ol_depth = 0;
                    }
                }
                b"nav" => nav_depth += 1,
                b"ol" if in_toc_nav => ol_depth += 1,
                b"a" if in_toc_nav && ol_depth > 0 => {
                    if let Some(href) = attr_value(&e, b"href") {
                        link = Some((href, String::new()));
                    }
                }
                _ => {}
            },
            Ok(Event::Text(t)) => {
                if let Some((_, title)) = &mut link {
                    if let Ok(text) = t.decode() {
                        push_word(title, &text);
                    }
                }
            }
            Ok(Event::GeneralRef(r)) => {
                if let Some((_, title)) = &mut link {
                    title.push_str(&resolve_ref(&r));
                }
            }
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"nav" if in_toc_nav => {
                    nav_depth -= 1;
                    if nav_depth == 0 {
                        break;
                    }
                }
                b"ol" if in_toc_nav && ol_depth > 0 => ol_depth -= 1,
                b"a" if in_toc_nav => {
                    if let Some((href, title)) = link.take() {
                        if let Some(title) = clean_text(Some(&title)) {
                            entries.push(TocEntry {
                                title,
                                href: resolve_href(nav_dir, &href),
                                depth: ol_depth.saturating_sub(1),
                            });
                        }
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    entries
}

// ---------------------------------------------------------------------------
// NCX fallback

/// Parses an NCX `<navMap>`: nested `<navPoint>` elements, each with a
/// `<navLabel><text>` title and a `<content src>` target.
fn parse_ncx(xml: &str, ncx_dir: &str) -> Vec<TocEntry> {
    let mut entries = Vec::new();
    let mut reader = Reader::from_str(xml);
    reader.config_mut().check_end_names = false;
    let mut buf = Vec::new();

    let mut labels: Vec<String> = Vec::new(); // one per open navPoint
    let mut in_text = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => match e.local_name().as_ref() {
                b"navPoint" => labels.push(String::new()),
                b"text" if !labels.is_empty() => in_text = true,
                b"content" => {
                    if let (Some(src), Some(title)) = (attr_value(&e, b"src"), labels.last()) {
                        if let Some(title) = clean_text(Some(title)) {
                            entries.push(TocEntry {
                                title,
                                href: resolve_href(ncx_dir, &src),
                                depth: (labels.len() as u32).saturating_sub(1),
                            });
                        }
                    }
                }
                _ => {}
            },
            Ok(Event::Text(t)) => {
                if in_text {
                    if let (Some(label), Ok(text)) = (labels.last_mut(), t.decode()) {
                        push_word(label, &text);
                    }
                }
            }
            Ok(Event::GeneralRef(r)) => {
                if in_text {
                    if let Some(label) = labels.last_mut() {
                        label.push_str(&resolve_ref(&r));
                    }
                }
            }
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"navPoint" => {
                    labels.pop();
                }
                b"text" => in_text = false,
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    entries
}

// ---------------------------------------------------------------------------
// Plain-text extraction (the search corpus)

/// Elements whose entire content is invisible to a reader.
const SKIPPED: &[&[u8]] = &[b"head", b"script", b"style", b"template"];
/// Elements that end a line of text.
const BLOCK: &[&[u8]] = &[
    b"p", b"div", b"h1", b"h2", b"h3", b"h4", b"h5", b"h6", b"li", b"blockquote", b"section",
    b"article", b"tr", b"caption", b"figcaption", b"dt", b"dd", b"pre",
];

/// Extracts whitespace-normalized plain text from an XHTML document:
/// newline per block element, single spaces inside a line. Returns `None`
/// when the document is malformed beyond quick-xml's tolerance.
fn extract_text(xml: &str) -> Option<String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().check_end_names = false;
    let mut buf = Vec::new();

    let mut out = String::new();
    let mut line = String::new();
    let mut skip_depth = 0u32;

    fn flush(out: &mut String, line: &mut String) {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            out.push_str(trimmed);
            out.push('\n');
        }
        line.clear();
    }

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                if SKIPPED.contains(&e.local_name().as_ref()) {
                    skip_depth += 1;
                }
            }
            Ok(Event::Empty(e)) => {
                if skip_depth == 0 && e.local_name().as_ref() == b"br" {
                    flush(&mut out, &mut line);
                }
            }
            Ok(Event::Text(t)) => {
                if skip_depth == 0 {
                    if let Ok(text) = t.decode() {
                        push_word(&mut line, &text);
                    }
                }
            }
            Ok(Event::CData(t)) => {
                if skip_depth == 0 {
                    if let Ok(text) = std::str::from_utf8(&t) {
                        push_word(&mut line, text);
                    }
                }
            }
            Ok(Event::GeneralRef(r)) => {
                if skip_depth == 0 {
                    line.push_str(&resolve_ref(&r));
                }
            }
            Ok(Event::End(e)) => {
                let name = e.local_name();
                if SKIPPED.contains(&name.as_ref()) {
                    skip_depth = skip_depth.saturating_sub(1);
                } else if skip_depth == 0 && (BLOCK.contains(&name.as_ref()) || name.as_ref() == b"br")
                {
                    flush(&mut out, &mut line);
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => return None,
            _ => {}
        }
        buf.clear();
    }
    flush(&mut out, &mut line);
    while out.ends_with('\n') {
        out.pop();
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// Small shared helpers

/// Resolves an entity-reference event: character references, the XML
/// predefined five, and (EPUB XHTML being HTML-flavored in practice) the
/// HTML5 named set. Unresolvable references are kept verbatim so no
/// content silently disappears.
fn resolve_ref(r: &quick_xml::events::BytesRef) -> String {
    if let Ok(Some(ch)) = r.resolve_char_ref() {
        return ch.to_string();
    }
    if let Ok(name) = r.decode() {
        if let Some(resolved) = quick_xml::escape::resolve_predefined_entity(&name)
            .or_else(|| quick_xml::escape::resolve_html5_entity(&name))
        {
            return resolved.to_string();
        }
        return format!("&{name};");
    }
    String::new()
}

/// Appends `text` to `acc`, collapsing runs of whitespace into single
/// spaces. UTF-8-safe: operates on `char` boundaries only, never byte
/// offsets. (Entities never appear here — the parser emits them as
/// separate `GeneralRef` events.)
fn push_word(acc: &mut String, text: &str) {
    let mut needs_sep = text.starts_with(char::is_whitespace);
    for word in text.split_whitespace() {
        if needs_sep && !acc.is_empty() && !acc.ends_with(' ') {
            acc.push(' ');
        }
        acc.push_str(word);
        needs_sep = true;
    }
    // Preserve a trailing separator when the raw text ended in whitespace,
    // so `push_word("a "), push_word("b")` stays two words — while text
    // split only by markup or entity boundaries joins seamlessly.
    if text.ends_with(char::is_whitespace) && !acc.is_empty() && !acc.ends_with(' ') {
        acc.push(' ');
    }
}

/// Trims a captured text value; `None` when empty.
fn clean_text(text: Option<&str>) -> Option<String> {
    let trimmed = text?.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// An attribute's entity-unescaped value by exact local-name match.
fn attr_value(e: &BytesStart, name: &[u8]) -> Option<String> {
    e.attributes().flatten().find_map(|attr| {
        if attr.key.local_name().as_ref() == name {
            attr.normalized_value(quick_xml::XmlVersion::default()).ok().map(|v| v.into_owned())
        } else {
            None
        }
    })
}

/// An attribute's value by raw qualified name (e.g. `epub:type`, where the
/// prefix is significant and `local_name` would collide with other vocab).
fn raw_attr(e: &BytesStart, name: &[u8]) -> Option<String> {
    e.attributes().flatten().find_map(|attr| {
        if attr.key.as_ref() == name {
            attr.normalized_value(quick_xml::XmlVersion::default()).ok().map(|v| v.into_owned())
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_hrefs_against_base_dirs() {
        assert_eq!(resolve_href("OEBPS", "ch01.xhtml"), "OEBPS/ch01.xhtml");
        assert_eq!(resolve_href("", "ch01.xhtml"), "ch01.xhtml");
        assert_eq!(resolve_href("OEBPS/text", "../images/c.png"), "OEBPS/images/c.png");
        assert_eq!(resolve_href("OEBPS", "./ch01.xhtml#s1"), "OEBPS/ch01.xhtml#s1");
        assert_eq!(resolve_href("OEBPS", "ch%201.xhtml"), "OEBPS/ch 1.xhtml");
        assert_eq!(resolve_href("OEBPS", "/root.xhtml"), "root.xhtml");
        // Fragment-only href targets the referencing document itself.
        assert_eq!(resolve_href("OEBPS/nav.xhtml", "#pt1"), "OEBPS/nav.xhtml#pt1");
    }

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

    #[test]
    fn nav_parsing_flattens_with_depth() {
        let xml = r#"<html xmlns:epub="http://www.idpf.org/2007/ops"><body>
<nav epub:type="lot"><ol><li><a href="x.xhtml">not the toc</a></li></ol></nav>
<nav epub:type="toc"><ol>
  <li><a href="ch01.xhtml">第一章</a>
    <ol><li><a href="ch01.xhtml#s1">第一節</a></li></ol>
  </li>
  <li><a href="ch02.xhtml">第二章</a></li>
</ol></nav>
</body></html>"#;
        let toc = parse_nav(xml, "OEBPS");
        assert_eq!(
            toc,
            vec![
                TocEntry { title: "第一章".into(), href: "OEBPS/ch01.xhtml".into(), depth: 0 },
                TocEntry { title: "第一節".into(), href: "OEBPS/ch01.xhtml#s1".into(), depth: 1 },
                TocEntry { title: "第二章".into(), href: "OEBPS/ch02.xhtml".into(), depth: 0 },
            ]
        );
    }

    #[test]
    fn ncx_parsing_flattens_with_depth() {
        let xml = r#"<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/"><navMap>
<navPoint id="a"><navLabel><text>第一章</text></navLabel><content src="ch01.xhtml"/>
  <navPoint id="b"><navLabel><text>第一節</text></navLabel><content src="ch01.xhtml#s1"/></navPoint>
</navPoint>
<navPoint id="c"><navLabel><text>第二章</text></navLabel><content src="ch02.xhtml"/></navPoint>
</navMap></ncx>"#;
        let toc = parse_ncx(xml, "OEBPS");
        assert_eq!(
            toc,
            vec![
                TocEntry { title: "第一章".into(), href: "OEBPS/ch01.xhtml".into(), depth: 0 },
                TocEntry { title: "第一節".into(), href: "OEBPS/ch01.xhtml#s1".into(), depth: 1 },
                TocEntry { title: "第二章".into(), href: "OEBPS/ch02.xhtml".into(), depth: 0 },
            ]
        );
    }
}
