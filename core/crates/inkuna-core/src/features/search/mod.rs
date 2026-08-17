//! Full-text search over the imported corpus (`resource_text`).
//!
//! Two engines, one corpus, chosen by what each question needs:
//!
//! - **In-book search** ([`Library::search_in_book`]) is an exact
//!   case-folded scan over one book's resources: every occurrence, char
//!   offsets for locators, partial-word Latin matches, single-Han-char
//!   matches. A tokenized index cannot answer "cat" inside "category" or
//!   enumerate offsets, and one book's corpus is small enough that a scan
//!   is milliseconds — so the index is deliberately not consulted here.
//! - **Library-wide search** ([`Library::search_all_books`]) is a ranked
//!   tantivy query over a persistent index in `<data_dir>/index/`: a
//!   jieba-segmented word field for scoring plus a CJK-unigram field with
//!   positions, so single-character and exact-substring CJK queries match
//!   across the whole library. Built from `resource_text` at import —
//!   never by re-parsing books — and reconciled against the database in
//!   the background on every open.

mod fold;
mod index;
mod model;
mod queries;
mod tokenize;

#[cfg(test)]
mod tests;

pub(crate) use index::SearchIndex;
pub use model::{BookSearchHit, BookSearchResults, LibrarySearchHit};
