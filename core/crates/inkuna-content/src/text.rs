//! Plain-text extraction (the search corpus): one whitespace-normalized
//! document per spine resource.

use std::collections::HashMap;
use std::fs::File;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use quick_xml::events::Event;
use quick_xml::Reader;
use rayon::prelude::*;

use crate::archive::read_spine_entry;
use crate::xml::{push_word, resolve_ref};
use crate::ContentError;

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
pub const MAX_TOTAL_TEXT_BYTES: usize = 32 * 1024 * 1024;

/// Elements whose entire content is invisible to a reader.
const SKIPPED: &[&[u8]] = &[b"head", b"script", b"style", b"template"];
/// Elements that end a line of text.
const BLOCK: &[&[u8]] = &[
    b"p",
    b"div",
    b"h1",
    b"h2",
    b"h3",
    b"h4",
    b"h5",
    b"h6",
    b"li",
    b"blockquote",
    b"section",
    b"article",
    b"tr",
    b"caption",
    b"figcaption",
    b"dt",
    b"dd",
    b"pre",
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
///
/// The corpus is a function of the publication alone: the same file
/// always yields the same texts, whatever rayon's scheduling did.
pub fn extract_spine_text(path: &Path, spine: &[String]) -> Vec<Option<Arc<str>>> {
    extract_spine_text_budgeted(path, spine, MAX_TOTAL_TEXT_BYTES)
}

/// One distinct resource's outcome from the parallel extraction pass.
enum Extraction {
    /// Text extracted, awaiting the budget decision.
    Text(String),
    /// Missing, unreadable, over the per-entry cap, or malformed. Never
    /// contributes text, and never charges the budget.
    Unusable,
    /// Never read, because the work guard had already tripped. The
    /// decision pass re-reads it if the budget still has room — which is
    /// what keeps the retained corpus independent of the interleaving.
    Deferred,
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

    // The atomic is a *work* guard, not the budget: it stops the
    // decompression once enough text to fill the budget already exists,
    // bounding the transient to roughly the budget plus one entry per
    // rayon worker. It deliberately decides nothing about what is kept,
    // because which threads win the race is not a property of the input —
    // charging retention here made two imports of one publication produce
    // different search corpora, and (the counter never being credited
    // back for a rejected text) permanently poisoned the pre-read check
    // so every later resource was dropped, however small.
    let extracted_bytes = AtomicUsize::new(0);
    let extracted: Vec<Extraction> = distinct
        .par_iter()
        .map_init(
            // One archive handle per rayon worker split; zip readers need
            // &mut access, so they cannot be shared across threads.
            || zip::ZipArchive::new(File::open(path)?).map_err(ContentError::from),
            |archive, href| {
                if extracted_bytes.load(Ordering::Relaxed) >= budget {
                    return Extraction::Deferred;
                }
                let archive = match archive.as_mut() {
                    Ok(archive) => archive,
                    Err(e) => {
                        log::warn!("cannot reopen {} for text extraction: {e}", path.display());
                        return Extraction::Unusable;
                    }
                };
                match extract_resource(archive, href, path) {
                    Some(text) => {
                        extracted_bytes.fetch_add(text.len(), Ordering::Relaxed);
                        Extraction::Text(text)
                    }
                    None => Extraction::Unusable,
                }
            },
        )
        .collect();

    // The decision pass: sequential, in `distinct` order, over the real
    // text lengths, so the retained set is a pure function of the
    // publication. Retention stops at the first text that does not fit,
    // the same rule the TOC budget follows — going on would mean re-reading
    // every deferred resource behind it, which is exactly the
    // decompression the guard above exists to stop.
    let mut warned = false;
    let mut deferred_archive = reopen_for_deferred(&extracted, path);
    let mut used = 0usize;
    let mut texts: Vec<Option<Arc<str>>> = Vec::with_capacity(distinct.len());
    for (at, outcome) in extracted.into_iter().enumerate() {
        let text = match outcome {
            Extraction::Text(text) => text,
            Extraction::Unusable => {
                texts.push(None);
                continue;
            }
            // Bounded: this runs only while the budget still has room and
            // the loop stops at the first text that does not fit, so the
            // bytes re-read here total at most the budget plus one entry.
            Extraction::Deferred => {
                let text = deferred_archive
                    .as_mut()
                    .and_then(|archive| extract_resource(archive, distinct[at], path));
                let Some(text) = text else {
                    texts.push(None);
                    continue;
                };
                text
            }
        };
        if used + text.len() > budget {
            warn_truncated(&mut warned, path, budget);
            break;
        }
        used += text.len();
        texts.push(Some(Arc::from(text)));
    }
    texts.resize(distinct.len(), None);

    // Expansion back to spine order charges the budget per *retained
    // copy*, not per distinct text: every `Some` here becomes its own
    // `resource_text` row, so a spine repeating one big chapter thousands
    // of times would otherwise multiply a within-budget extraction into
    // gigabytes of database while every per-entry cap held.
    let mut retained = 0usize;
    spine
        .iter()
        .map(|href| {
            let text = position
                .get(href.as_str())
                .and_then(|&at| texts[at].clone())?;
            if retained + text.len() > budget {
                warn_truncated(&mut warned, path, budget);
                return None;
            }
            retained += text.len();
            Some(text)
        })
        .collect()
}

/// Reads one spine resource and extracts its text. Skips *and logs*: a
/// resource dropped here — missing, unreadable, or past the per-entry
/// cap — is text the search corpus will never have, and a silent drop
/// leaves no way to tell a hostile chapter from an honest one.
fn extract_resource(
    archive: &mut zip::ZipArchive<File>,
    href: &str,
    path: &Path,
) -> Option<String> {
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
    Some(text)
}

/// Reopens the archive for the decision pass, and only when the guard
/// actually deferred something: an honest publication never spends its
/// budget, so it never pays for this second handle.
fn reopen_for_deferred(extracted: &[Extraction], path: &Path) -> Option<zip::ZipArchive<File>> {
    if !extracted.iter().any(|e| matches!(e, Extraction::Deferred)) {
        return None;
    }
    match File::open(path)
        .map_err(ContentError::from)
        .and_then(|file| zip::ZipArchive::new(file).map_err(ContentError::from))
    {
        Ok(archive) => Some(archive),
        Err(e) => {
            log::warn!(
                "cannot reopen {} for deferred text extraction: {e}",
                path.display()
            );
            None
        }
    }
}

/// Warns once per publication that the corpus was cut short. Exceeding the
/// budget drops the remaining resources rather than failing: the corpus is
/// an optional part, so this follows the parser's degrade-on-optional-part
/// convention and the import still succeeds.
fn warn_truncated(warned: &mut bool, path: &Path, budget: usize) {
    if !*warned {
        *warned = true;
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
                } else if skip_depth == 0
                    && (BLOCK.contains(&name.as_ref()) || name.as_ref() == b"br")
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
