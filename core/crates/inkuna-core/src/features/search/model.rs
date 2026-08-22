//! The search domain's result types.

use crate::Publication;

/// One occurrence of the query inside a book. The snippet arrives as
/// three pieces so a shell can highlight the match; `snippet_pre` /
/// `snippet_post` already carry a leading/trailing `…` when the window
/// truncated the surrounding text.
///
/// A hit's offset indexes its stored `resource_text` body. Once a book is
/// reconciled, that body is the canonical projection and
/// `Coordinate { spine_idx, char_offset }` feeds the reader session's
/// `locate` / `match_rects` directly. Plan-02 consumers must not feed an
/// unreconciled legacy-body offset to the engine.
#[derive(Debug, Clone, PartialEq)]
pub struct BookSearchHit {
    /// Reading-order index of the resource the hit is in.
    pub spine_idx: u32,
    /// The resource's package-relative href — with the in-resource
    /// `progression`, everything a shell needs to build a jump locator.
    pub href: String,
    /// Char (not byte) offset of the match in the resource's extracted
    /// text, counted in the original (unfolded) text.
    pub char_offset: u32,
    pub snippet_pre: String,
    pub snippet_match: String,
    pub snippet_post: String,
    /// Position of the hit within its resource, in [0, 1] — the value a
    /// Readium locator's `locations.progression` takes.
    pub progression: f64,
}

/// Every in-book occurrence up to the caller's cap, plus the true total.
#[derive(Debug, Clone, PartialEq)]
pub struct BookSearchResults {
    pub hits: Vec<BookSearchHit>,
    pub total: u32,
}

/// One book matching a library-wide query, best matches first. The
/// excerpt is the first in-text occurrence, in the same three-piece shape
/// as [`BookSearchHit`]'s snippet; a match the scan cannot pin to one
/// contiguous run (words spread across a resource) degrades to the
/// resource's opening text with an empty `excerpt_match`.
#[derive(Debug, Clone, PartialEq)]
pub struct LibrarySearchHit {
    pub publication: Publication,
    pub excerpt_pre: String,
    pub excerpt_match: String,
    pub excerpt_post: String,
}
