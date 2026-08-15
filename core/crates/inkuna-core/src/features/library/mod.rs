//! The library domain: the `Library` facade over one SQLite database plus
//! core-owned book and cover storage, the publication / chapter / bookmark
//! types, and the shelf, sort, search, and bookmark operations over them.

mod bookmarks;
mod model;
mod queries;
mod search;
mod store;

#[cfg(test)]
pub(crate) mod tests;

pub use model::{Bookmark, Chapter, Publication, Shelf, Sort};
pub(crate) use model::{join_authors, map_publication, PUB_COLUMNS};
pub use store::Library;
