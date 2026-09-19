//! The library domain: the `Library` facade over one SQLite database plus
//! core-owned book and cover storage, the publication / chapter / bookmark
//! types, and the shelf, sort, search, and bookmark operations over them.

mod bookmarks;
mod corpus;
mod edition;
mod edition_backfill;
mod model;
mod queries;
mod rebaseline;
mod search;
mod store;

#[cfg(test)]
mod tests;

pub(crate) use corpus::corpus_digest;
pub use edition::{edition_key, title_key};
pub use model::{Bookmark, Chapter, Publication, Shelf, Sort, SpineEntry};
pub(crate) use model::{join_authors, map_publication, PUB_COLUMNS};
pub use store::{Library, PUBLISHER_FONT_DIR};
