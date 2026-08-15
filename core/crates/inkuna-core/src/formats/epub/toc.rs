//! The table of contents: the EPUB 3 nav doc, with the EPUB 2 NCX as
//! fallback. Both flatten into the same depth-annotated entry list.

use quick_xml::events::Event;
use quick_xml::Reader;

use super::href::resolve_relative;
use super::model::TocEntry;
use super::xml::{attr_value, clean_text, push_word, raw_attr, resolve_ref};

/// Upper bound on the TOC entries kept for one publication. Each entry
/// becomes a persistent `chapters` row, so an uncapped crafted nav doc
/// turns a few hundred KB of archive into hundreds of MB of database
/// (measured: 385 KB → 480 MB, 2.5M rows). Real TOCs top out around a few
/// thousand entries; the TOC is an optional part, so the parse stops at
/// the cap and the rest degrades away with a warning.
pub(crate) const MAX_TOC_ENTRIES: usize = 10_000;

/// Upper bound on TOC nesting depth. Real TOCs nest a handful of levels;
/// in the NCX parser every open `<navPoint>` holds a label `String`, so a
/// crafted document nesting millions deep would grow that stack without
/// bound. Entries beyond the cap depth are skipped with a warning;
/// shallower ones still parse.
pub(crate) const MAX_TOC_DEPTH: usize = 64;

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
    let mut warned_depth = false;

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
                    if ol_depth as usize > MAX_TOC_DEPTH {
                        if !warned_depth {
                            warned_depth = true;
                            log::warn!(
                                "TOC in {nav_path} nests deeper than {MAX_TOC_DEPTH} levels; deeper entries skipped"
                            );
                        }
                    } else if let Some(href) = attr_value(&e, b"href") {
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
                            if entries.len() == MAX_TOC_ENTRIES {
                                log::warn!(
                                    "TOC in {nav_path} lists more than {MAX_TOC_ENTRIES} entries; truncated"
                                );
                                break;
                            }
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
    // Open navPoints beyond MAX_TOC_DEPTH: counted, never allocated, and
    // everything inside them is skipped.
    let mut overflow = 0usize;
    let mut in_text = false;
    let mut warned_depth = false;

    loop {
        let event = reader.read_event_into(&mut buf);
        match &event {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let is_start = matches!(&event, Ok(Event::Start(_)));
                match e.local_name().as_ref() {
                    // Only a Start opens a navPoint: a self-closed one can
                    // hold no label or target and never gets the End that
                    // would pop it, so pushing for it would leak one level
                    // of depth (and one String) per occurrence.
                    b"navPoint" if is_start => {
                        if labels.len() < MAX_TOC_DEPTH {
                            labels.push(String::new());
                        } else {
                            overflow += 1;
                            if !warned_depth {
                                warned_depth = true;
                                log::warn!(
                                    "TOC in {ncx_path} nests deeper than {MAX_TOC_DEPTH} levels; deeper entries skipped"
                                );
                            }
                        }
                    }
                    b"text" if overflow == 0 && !labels.is_empty() => in_text = true,
                    b"content" if overflow == 0 => {
                        if let (Some(src), Some(title)) = (attr_value(e, b"src"), labels.last()) {
                            if let Some(title) = clean_text(Some(title)) {
                                if entries.len() == MAX_TOC_ENTRIES {
                                    log::warn!(
                                        "TOC in {ncx_path} lists more than {MAX_TOC_ENTRIES} entries; truncated"
                                    );
                                    break;
                                }
                                entries.push(TocEntry {
                                    title,
                                    href: resolve_relative(ncx_path, &src),
                                    depth: (labels.len() as u32).saturating_sub(1),
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }
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
                        label.push_str(&resolve_ref(r));
                    }
                }
            }
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"navPoint" => {
                    if overflow > 0 {
                        overflow -= 1;
                    } else {
                        labels.pop();
                    }
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
