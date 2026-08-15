//! The table of contents: the EPUB 3 nav doc, with the EPUB 2 NCX as
//! fallback. Both flatten into the same depth-annotated entry list.

use quick_xml::events::Event;
use quick_xml::Reader;

use super::href::resolve_relative;
use super::model::TocEntry;
use super::xml::{attr_value, clean_text, push_word, raw_attr, resolve_ref};

/// Parses the `<nav epub:type="toc">` (or `role="doc-toc"`) of a nav doc
/// into a flattened, depth-annotated list. Nesting depth follows `<ol>`
/// levels; entries are `<a>` elements with an href.
pub(super) fn parse_nav(xml: &str, nav_path: &str) -> Vec<TocEntry> {
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
                                href: resolve_relative(nav_path, &href),
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

/// Parses an NCX `<navMap>`: nested `<navPoint>` elements, each with a
/// `<navLabel><text>` title and a `<content src>` target.
pub(super) fn parse_ncx(xml: &str, ncx_path: &str) -> Vec<TocEntry> {
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
                                href: resolve_relative(ncx_path, &src),
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

#[cfg(test)]
#[path = "toc_tests.rs"]
mod tests;
