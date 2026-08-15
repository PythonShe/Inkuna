//! The library's row types and the shared publication row mapping.

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
    pub format: Format,
    pub file_path: String,
    pub cover_path: Option<String>,
    pub added_at: i64,
    /// Book-wide `totalProgression` in [0, 1] — never per-resource.
    pub progression: f64,
    /// Current position: opaque Readium locator JSON, stored and returned,
    /// never parsed.
    pub locator: Option<String>,
    /// Readium synthetic position count, reported by the shell's navigator.
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
    /// Opened at least once and not finished.
    Reading,
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

/// A reader-placed mark in a publication. The core stores the locator and
/// the position it sorts by; what the row *shows* comes out of the locator,
/// which the core never interprets.
#[derive(Debug, Clone, PartialEq)]
pub struct Bookmark {
    pub id: String,
    pub publication_id: String,
    /// Opaque Readium locator JSON; carries the `href`/`title`/`text`
    /// context a bookmark row renders from.
    pub locator: String,
    pub progression: f64,
    pub created_at: i64,
}

pub(crate) const PUB_COLUMNS: &str = "id, title, authors, language, format, file_path, \
     cover_path, added_at, progression, locator, position_count, finished_at, last_opened_at";

pub(crate) fn map_publication(row: &rusqlite::Row) -> rusqlite::Result<Publication> {
    let format_str: String = row.get(4)?;
    let format = Format::from_str(&format_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let authors: String = row.get(2)?;
    Ok(Publication {
        id: row.get(0)?,
        title: row.get(1)?,
        authors: split_authors(&authors),
        language: row.get(3)?,
        format,
        file_path: row.get(5)?,
        cover_path: row.get(6)?,
        added_at: row.get(7)?,
        progression: row.get(8)?,
        locator: row.get(9)?,
        position_count: row.get(10)?,
        finished_at: row.get(11)?,
        last_opened_at: row.get(12)?,
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
