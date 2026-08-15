//! Plain-text extraction (the search corpus): one whitespace-normalized
//! document per spine resource.

use std::collections::HashMap;
use std::fs::File;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use quick_xml::events::Event;
use quick_xml::Reader;
use rayon::prelude::*;

use super::archive::read_spine_entry;
use super::xml::{push_word, resolve_ref};
use crate::CoreError;

/// Aggregate budget for the text retained for one publication. The
/// per-entry decompression cap bounds a single resource, never the corpus:
/// a spine may reference resources without limit, and the sum is what the
/// device actually pays — every retained byte becomes a persistent
/// `resource_text` row. A very large real novel's full text is a few MB,
/// and even omnibus "complete works" editions stay under ~20 MB, so
/// 32 MiB keeps real headroom over any honest book. The previous 128 MiB
/// was ~25x a huge novel, and measured, it let an 18 KB crafted archive
/// import into a 364 MB data directory; a crafted spine now stops at a
/// quarter of that.
const MAX_TOTAL_TEXT_BYTES: usize = 32 * 1024 * 1024;

/// Elements whose entire content is invisible to a reader.
const SKIPPED: &[&[u8]] = &[b"head", b"script", b"style", b"template"];
/// Elements that end a line of text.
const BLOCK: &[&[u8]] = &[
    b"p", b"div", b"h1", b"h2", b"h3", b"h4", b"h5", b"h6", b"li", b"blockquote", b"section",
    b"article", b"tr", b"caption", b"figcaption", b"dt", b"dd", b"pre",
];

/// Extracts plain text from every spine resource, in parallel across the
/// *distinct* resources. `None` marks a resource that is malformed,
/// missing, or past the aggregate budget — the import pipeline skips its
/// text row (logged) and still succeeds.
///
/// A spine may name the same resource any number of times; each repeat
/// reuses the single extraction (the returned `Arc`s alias), because a
/// resource cannot yield different text on a second reference. Results
/// come back in spine order, one entry per `itemref`.
pub fn extract_spine_text(path: &Path, spine: &[String]) -> Vec<Option<Arc<str>>> {
    extract_spine_text_budgeted(path, spine, MAX_TOTAL_TEXT_BYTES)
}

/// The real implementation, with the aggregate budget injectable so tests
/// can pin the budget's behavior without building a 128 MiB fixture.
/// Production always passes [`MAX_TOTAL_TEXT_BYTES`].
fn extract_spine_text_budgeted(
    path: &Path,
    spine: &[String],
    budget: usize,
) -> Vec<Option<Arc<str>>> {
    // First-occurrence order, so extraction still walks the archive
    // roughly in reading order.
    let mut distinct: Vec<&str> = Vec::new();
    let mut position: HashMap<&str, usize> = HashMap::with_capacity(spine.len());
    for href in spine {
        position.entry(href.as_str()).or_insert_with(|| {
            distinct.push(href.as_str());
            distinct.len() - 1
        });
    }

    let used = AtomicUsize::new(0);
    let warned = AtomicBool::new(false);
    let texts: Vec<Option<Arc<str>>> = distinct
        .par_iter()
        .map_init(
            // One archive handle per rayon worker split; zip readers need
            // &mut access, so they cannot be shared across threads.
            || zip::ZipArchive::new(File::open(path)?).map_err(CoreError::from),
            |archive, href| {
                // Checked before the read, so an exhausted budget also
                // stops the decompression, not just the retention.
                if used.load(Ordering::Relaxed) >= budget {
                    warn_truncated(&warned, path, budget);
                    return None;
                }
                let archive = match archive.as_mut() {
                    Ok(archive) => archive,
                    Err(e) => {
                        log::warn!("cannot reopen {} for text extraction: {e}", path.display());
                        return None;
                    }
                };
                // Skip *and log*: a resource dropped here — missing,
                // unreadable, or past the per-entry cap — is text the
                // search corpus will never have, and a silent drop leaves
                // no way to tell a hostile chapter from an honest one.
                let xml = match read_spine_entry(archive, href) {
                    Ok(xml) => xml,
                    Err(e) => {
                        log::warn!(
                            "skipping text extraction for {href} in {}: {e}",
                            path.display()
                        );
                        return None;
                    }
                };
                let Some(text) = extract_text(&xml) else {
                    log::warn!("skipping text extraction for malformed resource {href}");
                    return None;
                };
                if used.fetch_add(text.len(), Ordering::Relaxed) + text.len() > budget {
                    warn_truncated(&warned, path, budget);
                    return None;
                }
                Some(Arc::from(text))
            },
        )
        .collect();

    // Expansion back to spine order charges the budget per *retained
    // copy*, not per distinct text: every `Some` here becomes its own
    // `resource_text` row, so a spine repeating one big chapter thousands
    // of times would otherwise multiply a within-budget extraction into
    // gigabytes of database while every per-entry cap held.
    let mut retained = 0usize;
    spine
        .iter()
        .map(|href| {
            let text = position.get(href.as_str()).and_then(|&at| texts[at].clone())?;
            if retained + text.len() > budget {
                warn_truncated(&warned, path, budget);
                return None;
            }
            retained += text.len();
            Some(text)
        })
        .collect()
}

/// Warns once per publication that the corpus was cut short. Exceeding the
/// budget drops the remaining resources rather than failing: the corpus is
/// an optional part, so this follows the parser's degrade-on-optional-part
/// convention and the import still succeeds.
fn warn_truncated(warned: &AtomicBool, path: &Path, budget: usize) {
    if warned
        .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
        .is_ok()
    {
        log::warn!(
            "text extraction budget of {budget} bytes exhausted for {}; corpus truncated",
            path.display()
        );
    }
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
