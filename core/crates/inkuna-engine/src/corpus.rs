//! Corpus extraction: THE canonical projection at rest. Import and the
//! reconcile pass (M6) call [`extract_corpus`]; `resource_text` rows are
//! exactly this output, so search offsets and layout offsets index the
//! same stream BY CONSTRUCTION. Any change to what this yields is a
//! corpus-versioning event.

use std::path::Path;

use inkuna_content::{resolve_relative, split_fragment, ResourceReader};

use crate::dom::{parse, Document, StylesheetSource, MAX_STYLESHEET_BYTES};
use crate::style::{cap_sheet_sources, parse_sheet, resolve, Stylesheet};
use crate::text::project;

/// Per spine href, the resource's canonical projection text; `None` for
/// resources that fail closed (malformed, missing, unreadable).
///
/// The aggregate budget mirrors the old `extract_spine_text` contract:
/// retained text bytes are charged in spine order against
/// `max_total_bytes`, and a resource that would exceed it yields `None`
/// for itself and every later resource — deterministic, a function of
/// the publication alone, with no scheduling dependence. Duplicate
/// spine hrefs re-use the first extraction's result.
///
/// The archive is opened ONCE for the whole spine: this is the
/// rebaseline hot path, and re-parsing the zip central directory per
/// resource made it quadratic in the number of entries. An archive that
/// cannot be opened at all yields `None` for every resource — exactly
/// what per-resource reads produced before.
pub fn extract_corpus(
    epub_path: &Path,
    spine: &[String],
    max_total_bytes: usize,
) -> Vec<Option<String>> {
    let mut reader = match ResourceReader::open(epub_path) {
        Ok(reader) => reader,
        Err(e) => {
            log::debug!("corpus: {} unreadable ({e})", epub_path.display());
            return vec![None; spine.len()];
        }
    };
    let mut out: Vec<Option<String>> = Vec::with_capacity(spine.len());
    let mut used = 0usize;
    let mut tripped = false;
    for (i, href) in spine.iter().enumerate() {
        if tripped {
            out.push(None);
            continue;
        }
        if let Some(j) = spine[..i].iter().position(|h| h == href) {
            // A repeated spine entry aliases its first extraction and
            // charges the budget only once.
            out.push(out[j].clone());
            continue;
        }
        match extract_one(&mut reader, href) {
            None => out.push(None),
            Some(text) => {
                if used.saturating_add(text.len()) > max_total_bytes {
                    tripped = true;
                    out.push(None);
                } else {
                    used += text.len();
                    out.push(Some(text));
                }
            }
        }
    }
    out
}

/// One resource → its canonical projection text; any failure degrades
/// to `None`, never an error.
fn extract_one(reader: &mut ResourceReader, href: &str) -> Option<String> {
    let bytes = match reader.read(href) {
        Ok(bytes) => bytes,
        Err(e) => {
            log::debug!("corpus: {href} unreadable ({e})");
            return None;
        }
    };
    let doc = match parse(&bytes) {
        Ok(doc) => doc,
        Err(e) => {
            log::debug!("corpus: {href} failed to parse ({e})");
            return None;
        }
    };
    if doc.truncated {
        // The projection about to become this resource's canonical
        // `resource_text` is a prefix, or lost element structure, at a
        // parse budget. Never silent: a book whose corpus is short is
        // otherwise indistinguishable from a book that is short.
        log::warn!("corpus: {href} hit a parse budget; canonical text is degraded/truncated");
    }
    let sheets = chapter_stylesheets(reader, href, &doc);
    let styled = resolve(&doc, &sheets);
    Some(project(&styled).text)
}

/// A chapter's parsed stylesheet cascade — THE loading rules, shared
/// verbatim by the session worker so corpus and layout resolve styles
/// identically by construction: linked sheets read through the
/// container layer (resolved against the chapter's own path; an
/// unreadable sheet is skipped, logged), inline bodies verbatim, whole
/// sheets dropped from the END past [`MAX_STYLESHEET_BYTES`]. Reads go
/// through the caller's session-scoped [`ResourceReader`], so a book
/// whose every chapter links the same sheet pays one archive open, not
/// one per chapter.
pub(crate) fn chapter_stylesheets(
    reader: &mut ResourceReader,
    chapter_href: &str,
    doc: &Document,
) -> Vec<Stylesheet> {
    let mut css: Vec<String> = Vec::new();
    for source in &doc.stylesheets {
        match source {
            StylesheetSource::Inline(text) => css.push(text.clone()),
            StylesheetSource::Linked(href) => {
                let resolved = resolve_relative(chapter_href, href);
                let (path, _) = split_fragment(&resolved);
                match reader.read(path) {
                    Ok(bytes) => css.push(String::from_utf8_lossy(&bytes).into_owned()),
                    Err(e) => log::warn!("stylesheet skipped: {path} ({e})"),
                }
            }
        }
    }
    let refs: Vec<&str> = css.iter().map(String::as_str).collect();
    cap_sheet_sources(&refs, MAX_STYLESHEET_BYTES)
        .iter()
        .map(|c| parse_sheet(c))
        .collect()
}

#[cfg(test)]
#[path = "corpus_tests.rs"]
mod tests;
