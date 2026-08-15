//! Plain-text extraction (the search corpus): one whitespace-normalized
//! document per spine resource.

use std::fs::File;
use std::path::Path;

use quick_xml::events::Event;
use quick_xml::Reader;
use rayon::prelude::*;

use super::archive::read_entry;
use super::xml::{push_word, resolve_ref};
use crate::CoreError;

/// Elements whose entire content is invisible to a reader.
const SKIPPED: &[&[u8]] = &[b"head", b"script", b"style", b"template"];
/// Elements that end a line of text.
const BLOCK: &[&[u8]] = &[
    b"p", b"div", b"h1", b"h2", b"h3", b"h4", b"h5", b"h6", b"li", b"blockquote", b"section",
    b"article", b"tr", b"caption", b"figcaption", b"dt", b"dd", b"pre",
];

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

#[cfg(test)]
#[path = "text_tests.rs"]
mod tests;
