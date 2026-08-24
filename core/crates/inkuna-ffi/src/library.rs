//! Library records — publications, chapters, bookmarks — and the shelf
//! queries over them.

use crate::bookshelf::blocking;
use crate::error::InkunaError;
use crate::format::Format;
use crate::reader::Coordinate;

/// Which subset of the library a listing covers. Filtering happens in
/// SQL, so an excluded book is never fetched — pick the shelf that
/// matches the screen instead of listing `All` and filtering shell-side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Shelf {
    /// Opened at least once and not finished — "continue reading". A
    /// freshly imported book is deliberately absent.
    Reading,
    /// Everything not finished, opened or not: what a library screen lists,
    /// so an imported book is visible immediately.
    Unfinished,
    /// Explicitly marked finished (the row carries a finish timestamp).
    Finished,
    /// Every imported book, finished or not.
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

/// Row order for a listing. Both are descending — newest first — and
/// both are applied in SQL, so the returned `Vec` is already in display
/// order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Sort {
    /// Most recently opened first, so tonight's hero is the first row.
    /// Never-opened books sort last.
    RecentlyOpened,
    /// Most recently imported first — import order, independent of
    /// whether the book has ever been opened.
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

/// One entry of the book's flattened table of contents. The TOC tree is
/// flattened in document order and `depth` is what remains of its
/// nesting, so a shell renders the list as-is and indents by `depth`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Chapter {
    /// Stable row id for this entry, unique across the library; not the
    /// EPUB's own element id.
    pub id: String,
    /// Position in this flattened list, in document order — an index
    /// into the `chapters()` result, NOT a spine index. Resolve the
    /// resource through `href`, never through this.
    pub idx: u32,
    pub title: String,
    /// Package-root-relative engine locate_href target, possibly with fragment.
    pub href: String,
    /// Nesting level in the original TOC tree; 0 for a top-level entry.
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

/// One spine resource in reading order — the `spine_idx` →
/// resource-href map that makes a stored `Coordinate` interpretable
/// without an open `ReaderSession` (naming the current chapter on a Home
/// or Detail screen).
///
/// `href` is package-root-relative and fragment-free: the same string a
/// `ReaderSession.locate_href` call takes, and the one a `Chapter.href`
/// matches once its fragment is stripped. Entry `n` always has
/// `spine_idx == n`, so the list may be indexed directly by a
/// coordinate's `spine_idx` after a bounds check.
#[derive(Debug, Clone, uniffi::Record)]
pub struct SpineEntry {
    /// Reading-order index; exactly what `Coordinate.spine_idx` holds.
    pub spine_idx: u32,
    pub href: String,
}

impl From<inkuna_core::SpineEntry> for SpineEntry {
    fn from(e: inkuna_core::SpineEntry) -> Self {
        SpineEntry {
            spine_idx: e.spine_idx,
            href: e.href,
        }
    }
}

/// One user-pinned position in a book. Bookmarks are core-owned rows:
/// they survive re-layout and setting changes because they store a
/// content coordinate, not a page number.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Bookmark {
    /// Stable row id — the handle `remove_bookmark` takes.
    pub id: String,
    pub publication_id: String,
    /// The pinned position, or `None` when the row has no stored
    /// coordinate — a legacy row the rebaseline has not converted yet, or
    /// one pinned before the shell had an engine coordinate. Fall back to
    /// `progression`; never treat it as book start.
    pub coordinate: Option<Coordinate>,
    pub progression: f64,
    pub created_at: i64,
}

impl From<inkuna_core::Bookmark> for Bookmark {
    fn from(b: inkuna_core::Bookmark) -> Self {
        Bookmark {
            id: b.id,
            publication_id: b.publication_id,
            coordinate: b.coordinate.map(Into::into),
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
    /// The `shelf` subset in `sort` order, ready to render: the rows come
    /// back already ordered, and `file_path`/`cover_path` already
    /// absolutized against the data dir. An empty library is an empty
    /// `Vec`, never an error.
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

    /// One publication by id, with absolutized paths. Throws `NotFound`
    /// when the row is gone — expect that whenever a stale id survives a
    /// removal on another screen.
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

    /// The publication's spine in reading order, so a screen with no
    /// open reader can resolve a stored `Coordinate.spine_idx` to a
    /// resource href (and from there to a TOC title by matching
    /// `Chapter.href` minus its fragment).
    ///
    /// Deliberately a separate list rather than a field on `Chapter`:
    /// the TOC-to-spine mapping is lossy both ways — a chapter whose
    /// href matches no spine resource has no spine index, and one
    /// chapter may cover several spine items — so a `spineIdx` on
    /// `Chapter` could not answer this honestly. Throws `NotFound` for
    /// an unknown id.
    pub async fn spine(&self, id: String) -> Result<Vec<SpineEntry>, InkunaError> {
        let library = self.0.clone();
        blocking(move || Ok(library.spine(&id)?.into_iter().map(Into::into).collect())).await
    }

    /// Pins a mark at `coordinate`. Pass `coordinate: None` when the
    /// caller has no engine coordinate: the row then stores no coordinate
    /// at all instead of a book-start placeholder, and `progression`
    /// carries the position.
    pub async fn add_bookmark(
        &self,
        id: String,
        coordinate: Option<Coordinate>,
        progression: f64,
    ) -> Result<Bookmark, InkunaError> {
        let library = self.0.clone();
        blocking(move || {
            Ok(library
                .add_bookmark(&id, coordinate.map(Into::into), progression)?
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

    /// Removes one bookmark by its row id. Not idempotent: an id that no
    /// longer exists throws `NotFound`, so a double-tap or a retry after
    /// a successful delete must be swallowed shell-side.
    pub async fn remove_bookmark(&self, bookmark_id: String) -> Result<(), InkunaError> {
        let library = self.0.clone();
        blocking(move || Ok(library.remove_bookmark(&bookmark_id)?)).await
    }
}
