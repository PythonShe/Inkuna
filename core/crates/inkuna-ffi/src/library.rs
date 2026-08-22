//! Library records — publications, chapters, bookmarks — and the shelf
//! queries over them.

use crate::bookshelf::blocking;
use crate::error::InkunaError;
use crate::format::Format;
use crate::reader::Coordinate;

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Shelf {
    /// Opened at least once and not finished — "continue reading". A
    /// freshly imported book is deliberately absent.
    Reading,
    /// Everything not finished, opened or not: what a library screen lists,
    /// so an imported book is visible immediately.
    Unfinished,
    Finished,
    All,
}

impl From<Shelf> for inkuna_core::Shelf {
    fn from(s: Shelf) -> Self {
        match s {
            Shelf::Reading => inkuna_core::Shelf::Reading,
            Shelf::Unfinished => inkuna_core::Shelf::Unfinished,
            Shelf::Finished => inkuna_core::Shelf::Finished,
            Shelf::All => inkuna_core::Shelf::All,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Sort {
    /// Tonight's hero is the first row.
    RecentlyOpened,
    RecentlyAdded,
}

impl From<Sort> for inkuna_core::Sort {
    fn from(s: Sort) -> Self {
        match s {
            Sort::RecentlyOpened => inkuna_core::Sort::RecentlyOpened,
            Sort::RecentlyAdded => inkuna_core::Sort::RecentlyAdded,
        }
    }
}

/// `file_path`/`cover_path` are absolute here (relative in the DB).
#[derive(Debug, Clone, uniffi::Record)]
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
    /// Book-wide `totalProgression` in [0, 1].
    pub progression: f64,
    /// Current reading position; `None` until the first progress write
    /// (legacy rows also surface `None` until the rebaseline converts
    /// them).
    pub coordinate: Option<Coordinate>,
    /// Core-computed synthetic position count.
    pub position_count: Option<u32>,
    pub finished_at: Option<i64>,
    pub last_opened_at: Option<i64>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct Chapter {
    pub id: String,
    pub idx: u32,
    pub title: String,
    /// Package-root-relative Readium jump target, possibly with fragment.
    pub href: String,
    pub depth: u32,
}

impl From<inkuna_core::Chapter> for Chapter {
    fn from(c: inkuna_core::Chapter) -> Self {
        Chapter {
            id: c.id,
            idx: c.idx,
            title: c.title,
            href: c.href,
            depth: c.depth,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct Bookmark {
    pub id: String,
    pub publication_id: String,
    /// The pinned position; a legacy row the rebaseline has not
    /// converted yet reads as the book-start default `(0, 0)`.
    pub coordinate: Coordinate,
    pub progression: f64,
    pub created_at: i64,
}

impl From<inkuna_core::Bookmark> for Bookmark {
    fn from(b: inkuna_core::Bookmark) -> Self {
        Bookmark {
            id: b.id,
            publication_id: b.publication_id,
            coordinate: b.coordinate.into(),
            progression: b.progression,
            created_at: b.created_at,
        }
    }
}

/// Absolutizes DB-relative paths against the library's data dir.
pub(crate) fn publication_record(
    library: &inkuna_core::Library,
    p: inkuna_core::Publication,
) -> Publication {
    let data_dir = library.data_dir();
    Publication {
        file_path: data_dir.join(&p.file_path).to_string_lossy().into_owned(),
        cover_path: p
            .cover_path
            .as_deref()
            .map(|c| data_dir.join(c).to_string_lossy().into_owned()),
        id: p.id,
        title: p.title,
        authors: p.authors,
        language: p.language,
        text_encoding: p.text_encoding,
        format: p.format.into(),
        added_at: p.added_at,
        progression: p.progression,
        coordinate: p.coordinate.map(Into::into),
        position_count: p.position_count,
        finished_at: p.finished_at,
        last_opened_at: p.last_opened_at,
    }
}

/// The library facade: publication records, chapters, and bookmarks.
/// Constructed once by [`Bookshelf::open`], handed out by
/// `Bookshelf::library()` as a cheap `Arc` clone.
#[derive(uniffi::Object)]
pub struct ShelfLibrary(pub(crate) std::sync::Arc<inkuna_core::Library>);

#[uniffi::export(async_runtime = "tokio")]
impl ShelfLibrary {
    pub async fn list(&self, shelf: Shelf, sort: Sort) -> Result<Vec<Publication>, InkunaError> {
        let library = self.0.clone();
        blocking(move || {
            let publications = library.list(shelf.into(), sort.into())?;
            Ok(publications
                .into_iter()
                .map(|p| publication_record(&library, p))
                .collect())
        })
        .await
    }

    pub async fn publication(&self, id: String) -> Result<Publication, InkunaError> {
        let library = self.0.clone();
        blocking(move || {
            let publication = library.publication(&id)?;
            Ok(publication_record(&library, publication))
        })
        .await
    }

    /// Removes the publication row (bookmarks, sessions, chapters, and
    /// text cascade), its book file, and its cover.
    pub async fn remove(&self, id: String) -> Result<(), InkunaError> {
        let library = self.0.clone();
        blocking(move || Ok(library.remove(&id)?)).await
    }

    /// Case-folded, CJK-safe substring search over title + authors.
    pub async fn search_library(&self, query: String) -> Result<Vec<Publication>, InkunaError> {
        let library = self.0.clone();
        blocking(move || {
            let publications = library.search_library(&query)?;
            Ok(publications
                .into_iter()
                .map(|p| publication_record(&library, p))
                .collect())
        })
        .await
    }

    /// The flattened TOC in document order; empty when the book has none.
    pub async fn chapters(&self, id: String) -> Result<Vec<Chapter>, InkunaError> {
        let library = self.0.clone();
        blocking(move || Ok(library.chapters(&id)?.into_iter().map(Into::into).collect())).await
    }

    pub async fn add_bookmark(
        &self,
        id: String,
        coordinate: Coordinate,
        progression: f64,
    ) -> Result<Bookmark, InkunaError> {
        let library = self.0.clone();
        blocking(move || {
            Ok(library
                .add_bookmark(&id, coordinate.into(), progression)?
                .into())
        })
        .await
    }

    /// Bookmarks sorted by progression through the book.
    pub async fn bookmarks(&self, id: String) -> Result<Vec<Bookmark>, InkunaError> {
        let library = self.0.clone();
        blocking(move || {
            Ok(library
                .bookmarks(&id)?
                .into_iter()
                .map(Into::into)
                .collect())
        })
        .await
    }

    pub async fn remove_bookmark(&self, bookmark_id: String) -> Result<(), InkunaError> {
        let library = self.0.clone();
        blocking(move || Ok(library.remove_bookmark(&bookmark_id)?)).await
    }
}
