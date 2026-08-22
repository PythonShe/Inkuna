//! The library's row types and the shared publication row mapping.

use inkuna_engine::Coordinate;

use crate::Format;

/// Authors are stored joined by the ASCII unit separator: it cannot appear
/// in real names, unlike commas or semicolons (common in CJK author lists).
const AUTHOR_SEP: char = '\u{1f}';

/// A library publication. `file_path` and `cover_path` are relative to the
/// library's data dir (iOS container paths change across installs, so
/// absolute paths are never persisted); the FFI layer absolutizes on read.
#[derive(Debug, Clone, PartialEq)]
pub struct Publication {
    pub id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub language: Option<String>,
    /// Source charset for normalized plain-text imports; native EPUBs use `None`.
    pub text_encoding: Option<String>,
    pub format: Format,
    pub file_path: String,
    pub cover_path: Option<String>,
    pub added_at: i64,
    /// Book-wide `totalProgression` in [0, 1] — never per-resource.
    pub progression: f64,
    /// Current reading position as a content coordinate; `None` until the
    /// first progress write (pre-reconcile rows also surface `None` while
    /// the V8 rebaseline is still converting their legacy locator).
    pub coordinate: Option<Coordinate>,
    /// Core-computed synthetic position count over the canonical
    /// projection.
    pub position_count: Option<u32>,
    /// Non-NULL = on the Finished shelf; powers "books this year".
    pub finished_at: Option<i64>,
    pub last_opened_at: Option<i64>,
}

/// One entry of the flattened TOC. `href` (which may carry a fragment) is
/// the Readium jump target; mapping a chapter to its resource is
/// href-minus-fragment matched against the spine, derived at query time
/// when needed — never stored.
#[derive(Debug, Clone, PartialEq)]
pub struct Chapter {
    pub id: String,
    pub idx: u32,
    pub title: String,
    pub href: String,
    pub depth: u32,
}

/// Library shelves, filtered server-side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shelf {
    /// Opened at least once and not finished — what "continue reading"
    /// means. A freshly imported book is deliberately absent.
    Reading,
    /// Everything not finished, opened or not. This is the shelf a library
    /// screen wants: a book must be visible the moment it is imported,
    /// which `Reading` cannot express without losing its own meaning.
    Unfinished,
    Finished,
    All,
}

/// List orderings. (A `Title` sort is deliberately omitted — no shell
/// affordance exists; enum variants are additive later.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    /// Tonight's hero is the first row.
    RecentlyOpened,
    RecentlyAdded,
}

/// A reader-placed mark in a publication: a content coordinate plus the
/// book-wide progression the list sorts by. A legacy row whose
/// coordinate the V8 rebaseline has not converted yet reads as the
/// book-start default `(0, 0)`.
#[derive(Debug, Clone, PartialEq)]
pub struct Bookmark {
    pub id: String,
    pub publication_id: String,
    pub coordinate: Coordinate,
    pub progression: f64,
    pub created_at: i64,
}

pub(crate) const PUB_COLUMNS: &str =
    "id, title, authors, language, text_encoding, format, file_path, \
     cover_path, added_at, progression, position_spine_idx, position_char_offset, \
     position_count, finished_at, last_opened_at";

pub(crate) fn map_publication(row: &rusqlite::Row) -> rusqlite::Result<Publication> {
    let format_str: String = row.get(5)?;
    let format = Format::from_str(&format_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let authors: String = row.get(2)?;
    // Both coordinate columns non-NULL → a position exists; anything else
    // (fresh book, or a legacy row the rebaseline has not converted yet)
    // surfaces `None`.
    let spine_idx: Option<u32> = row.get(10)?;
    let char_offset: Option<i64> = row.get(11)?;
    let coordinate = match (spine_idx, char_offset) {
        (Some(spine_idx), Some(char_offset)) => Some(Coordinate {
            spine_idx,
            char_offset: char_offset.max(0) as u64,
        }),
        _ => None,
    };
    Ok(Publication {
        id: row.get(0)?,
        title: row.get(1)?,
        authors: split_authors(&authors),
        language: row.get(3)?,
        text_encoding: row.get(4)?,
        format,
        file_path: row.get(6)?,
        cover_path: row.get(7)?,
        added_at: row.get(8)?,
        progression: row.get(9)?,
        coordinate,
        position_count: row.get(12)?,
        finished_at: row.get(13)?,
        last_opened_at: row.get(14)?,
    })
}

pub(crate) fn join_authors(authors: &[String]) -> String {
    authors.join(&AUTHOR_SEP.to_string())
}

fn split_authors(joined: &str) -> Vec<String> {
    if joined.is_empty() {
        Vec::new()
    } else {
        joined.split(AUTHOR_SEP).map(str::to_string).collect()
    }
}
