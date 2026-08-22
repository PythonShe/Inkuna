//! The table of contents: the EPUB 3 nav doc, with the EPUB 2 NCX as
//! fallback. Both flatten into the same depth-annotated entry list.

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::href::resolve_relative;
use crate::model::TocEntry;
use crate::xml::{attr_value, clean_text, push_word, raw_attr, resolve_ref};

/// Upper bound on the TOC entries kept for one publication. With
/// [`MAX_TOC_TOTAL_BYTES`] bounding the retained bytes, this only bounds
/// the per-row constant overhead (row ids, indexes) the byte budget does
/// not count. It sits far above the largest honest TOCs — verse-level
/// Bibles, dictionaries, and legal codes legitimately run past 30,000
/// entries — because the TOC is an optional part that degrades: the parse
/// stops at the cap and the rest falls away with a warning, which for a
/// real book is its own kind of data loss.
pub const MAX_TOC_ENTRIES: usize = 100_000;

/// Aggregate byte budget for the TOC retained from one publication,
/// charging what each entry actually keeps: its title plus its resolved
/// href. Per-entry and per-document caps cannot bound the *product* of
/// in-bounds factors — a 64 KiB label emitted once per crafted repeat
/// passed every existing cap while persisting 630 MB of `chapters` rows
/// out of a 2 KB archive. The most extreme honest TOC known (a
/// verse-level Bible, ~31,000 entries) retains under 2 MB, so 8 MiB keeps
/// several times that headroom while a crafted TOC stops here instead of
/// at hundreds of MB. Accounting uses whole-`String` byte lengths only;
/// no text is ever split or indexed at a byte offset, so multi-byte CJK
/// titles are never cut.
pub const MAX_TOC_TOTAL_BYTES: usize = 8 * 1024 * 1024;

/// Appends `entry` when both TOC caps allow it: the entry-count cap and
/// the aggregate byte budget over what each entry retains. Returns
/// `false` — after logging one warning — when a cap is hit; the caller
/// stops parsing, since the caps only ever tighten and nothing later
/// could be kept either.
fn push_entry(
    entries: &mut Vec<TocEntry>,
    retained: &mut usize,
    entry: TocEntry,
    doc_path: &str,
) -> bool {
    if entries.len() == MAX_TOC_ENTRIES {
        log::warn!("TOC in {doc_path} lists more than {MAX_TOC_ENTRIES} entries; truncated");
        return false;
    }
    let bytes = entry.title.len() + entry.href.len();
    if *retained + bytes > MAX_TOC_TOTAL_BYTES {
        log::warn!("TOC in {doc_path} exceeds the {MAX_TOC_TOTAL_BYTES}-byte budget; truncated");
        return false;
    }
    *retained += bytes;
    entries.push(entry);
    true
}

/// Upper bound on TOC nesting depth. Real TOCs nest a handful of levels;
/// in the NCX parser every open `<navPoint>` holds a label `String`, so a
/// crafted document nesting millions deep would grow that stack without
/// bound. Entries beyond the cap depth are skipped with a warning;
/// shallower ones still parse.
pub(crate) const MAX_TOC_DEPTH: usize = 64;

/// Parses the `<nav epub:type="toc">` (or `role="doc-toc"`) of a nav doc
/// into a flattened, depth-annotated list. Nesting depth follows `<ol>`
/// levels; entries are `<a>` elements with an href.
pub(crate) fn parse_nav(xml: &str, nav_path: &str) -> Vec<TocEntry> {
    let mut entries = Vec::new();
    let mut reader = Reader::from_str(xml);
    reader.config_mut().check_end_names = false;
    let mut buf = Vec::new();

    let mut in_toc_nav = false;
    let mut nav_depth = 0u32; // <nav> nesting while inside the toc nav
    let mut ol_depth = 0u32;
    let mut link: Option<(String, String)> = None; // (href, title so far)
    let mut warned_depth = false;
    let mut retained = 0usize; // bytes charged against MAX_TOC_TOTAL_BYTES

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
                            let entry = TocEntry {
                                title,
                                href: resolve_relative(nav_path, &href),
                                depth: ol_depth.saturating_sub(1),
                            };
                            if !push_entry(&mut entries, &mut retained, entry, nav_path) {
                                break;
                            }
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
pub(crate) fn parse_ncx(xml: &str, ncx_path: &str) -> Vec<TocEntry> {
    let mut entries = Vec::new();
    let mut reader = Reader::from_str(xml);
    reader.config_mut().check_end_names = false;
    let mut buf = Vec::new();

    // One `(label, content already taken)` pair per open navPoint.
    let mut labels: Vec<(String, bool)> = Vec::new();
    // Open navPoints beyond MAX_TOC_DEPTH: counted, never allocated, and
    // everything inside them is skipped.
    let mut overflow = 0usize;
    let mut in_text = false;
    let mut warned_depth = false;
    let mut retained = 0usize; // bytes charged against MAX_TOC_TOTAL_BYTES

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
                            labels.push((String::new(), false));
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
                    // Only a Start opens a label, for the same reason a
                    // Start alone opens a navPoint: `<text/>` carries no
                    // character data and never gets the End that clears
                    // the flag, so treating it as an opening tag left
                    // `in_text` stuck — and every later character node,
                    // including data outside any `<text>`, was appended to
                    // whichever label happened to be open.
                    b"text" if is_start && overflow == 0 && !labels.is_empty() => in_text = true,
                    // NCX (Z39.86-2005) fixes a navPoint's content model to
                    // `(navLabel+, content, navPoint*)`: exactly one
                    // <content>. Only the first wins — before the `taken`
                    // guard, every repeated <content> re-cloned the whole
                    // label, so one 64 KiB label × 10,000 repeats persisted
                    // 630 MB of `chapters` rows out of a 2 KB archive while
                    // each per-entry cap saw in-bounds numbers.
                    b"content" if overflow == 0 => {
                        let depth = (labels.len() as u32).saturating_sub(1);
                        if let (Some(src), Some((title, taken))) =
                            (attr_value(e, b"src"), labels.last_mut())
                        {
                            if !*taken {
                                *taken = true;
                                if let Some(title) = clean_text(Some(title.as_str())) {
                                    let entry = TocEntry {
                                        title,
                                        href: resolve_relative(ncx_path, &src),
                                        depth,
                                    };
                                    if !push_entry(&mut entries, &mut retained, entry, ncx_path) {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(t)) => {
                if in_text {
                    if let (Some((label, _)), Ok(text)) = (labels.last_mut(), t.decode()) {
                        push_word(label, &text);
                    }
                }
            }
            Ok(Event::GeneralRef(r)) => {
                if in_text {
                    if let Some((label, _)) = labels.last_mut() {
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
