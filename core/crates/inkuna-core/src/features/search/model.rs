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
/// `locate` / `match_rects` directly. Whether that holds is reported by
/// [`BookSearchResults::canonical`]; consumers gate on that flag rather
/// than assuming it.
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
    /// legacy locator's `locations.progression` takes.
    pub progression: f64,
}

/// Every in-book occurrence up to the caller's cap, plus the true total.
#[derive(Debug, Clone, PartialEq)]
pub struct BookSearchResults {
    pub hits: Vec<BookSearchHit>,
    pub total: u32,
    /// Whether these offsets index the canonical projection — i.e. the
    /// book's `reconciled_at` is set. `false` means the scan ran over a
    /// legacy-extractor body that the background V8 rebaseline has not
    /// replaced yet: the snippets are still correct to show, but the
    /// offsets must not be handed to the engine (`locate` /
    /// `match_rects`) because they do not address the same text.
    pub canonical: bool,
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
