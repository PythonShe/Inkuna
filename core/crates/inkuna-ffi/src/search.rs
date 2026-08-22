//! Full-text search: exact in-book occurrence scan and ranked
//! library-wide queries over the tantivy index.

use crate::bookshelf::blocking;
use crate::error::InkunaError;
use crate::library::{publication_record, Publication};

/// One occurrence of the query inside a book. `snippet_pre` /
/// `snippet_post` already carry a leading/trailing `…` when truncated;
/// the three-piece shape lets a row highlight `snippet_match`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BookSearchHit {
    /// Reading-order index of the resource the hit is in.
    pub spine_idx: u32,
    /// The resource's package-relative href; with `progression`, what a
    /// shell needs to build a jump locator.
    pub href: String,
    /// Char (not byte) offset of the match in the resource's text.
    pub char_offset: u32,
    pub snippet_pre: String,
    pub snippet_match: String,
    pub snippet_post: String,
    /// In-resource position of the hit, in [0, 1] — the value a Readium
    /// locator's `locations.progression` takes.
    pub progression: f64,
}

impl From<inkuna_core::BookSearchHit> for BookSearchHit {
    fn from(h: inkuna_core::BookSearchHit) -> Self {
        BookSearchHit {
            spine_idx: h.spine_idx,
            href: h.href,
            char_offset: h.char_offset,
            snippet_pre: h.snippet_pre,
            snippet_match: h.snippet_match,
            snippet_post: h.snippet_post,
            progression: h.progression,
        }
    }
}

/// Hits up to the requested cap, in reading order, plus the true total.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BookSearchResults {
    pub hits: Vec<BookSearchHit>,
    pub total: u32,
}

/// One book matching a library-wide query, best first, with the first
/// pinnable excerpt in the same three-piece shape as a snippet. An
/// `excerpt_match` may be empty when word matches spread across a
/// resource; the excerpt then shows the resource's opening text.
#[derive(Debug, Clone, uniffi::Record)]
pub struct LibrarySearchHit {
    pub publication: Publication,
    pub excerpt_pre: String,
    pub excerpt_match: String,
    pub excerpt_post: String,
}

/// The search facade: in-book scan and library-wide ranked search.
/// Constructed once by [`Bookshelf::open`], handed out by
/// `Bookshelf::search()` as a cheap `Arc` clone.
#[derive(uniffi::Object)]
pub struct ShelfSearch(pub(crate) std::sync::Arc<inkuna_core::Library>);

#[uniffi::export(async_runtime = "tokio")]
impl ShelfSearch {
    /// Every occurrence of `query` in one book, in reading order: an
    /// exact case-folded substring scan — partial words and single CJK
    /// chars match by construction. Any minimum-length floor is shell UI
    /// policy; the core searches any non-empty query.
    pub async fn search_in_book(
        &self,
        id: String,
        query: String,
        limit: u32,
    ) -> Result<BookSearchResults, InkunaError> {
        let library = self.0.clone();
        blocking(move || {
            let results = library.search_in_book(&id, &query, limit)?;
            Ok(BookSearchResults {
                hits: results.hits.into_iter().map(Into::into).collect(),
                total: results.total,
            })
        })
        .await
    }

    /// Ranked full-text search across every book in the library: jieba
    /// word matching for ranking, exact CJK unigram phrases so single
    /// characters and any CJK substring match. One excerpt per book.
    pub async fn search_all_books(
        &self,
        query: String,
        limit: u32,
    ) -> Result<Vec<LibrarySearchHit>, InkunaError> {
        let library = self.0.clone();
        blocking(move || {
            let hits = library.search_all_books(&query, limit)?;
            Ok(hits
                .into_iter()
                .map(|h| LibrarySearchHit {
                    publication: publication_record(&library, h.publication),
                    excerpt_pre: h.excerpt_pre,
                    excerpt_match: h.excerpt_match,
                    excerpt_post: h.excerpt_post,
                })
                .collect())
        })
        .await
    }
}
