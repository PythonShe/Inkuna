//! Reading-progress writes and the chapter position ranges derived from
//! the core-computed synthetic positions.

use crate::bookshelf::blocking;
use crate::error::InkunaError;
use crate::reader::Coordinate;

/// One TOC entry's span of synthetic positions; 1-based, both bounds
/// inclusive.
///
/// Rows are **per TOC chapter, not per spine item, and sparse**:
///
/// - `chapter_idx` is a `Chapter.idx` — an index into the `chapters()`
///   result — never a spine index and never a row index into this
///   vector.
/// - The vector is **sparse**: a chapter whose href matches no spine
///   resource has no row at all, so `rows.len()` is NOT the chapter
///   count and emphatically NOT the spine count. Never index this vector
///   positionally; match on `chapter_idx`.
/// - Spans are derived from the reading order, not the TOC order: a
///   chapter covering several spine items spans all of them, and
///   fragment-anchored chapters sharing one resource each report that
///   whole resource — so spans of sibling chapters may overlap.
/// - The vector is empty until the book's synthetic positions are
///   computed, and for a book with no TOC.
#[derive(Debug, Clone, Copy, uniffi::Record)]
pub struct ChapterPositionRange {
    pub chapter_idx: u32,
    pub start_position: u32,
    pub end_position: u32,
}

impl From<inkuna_core::ChapterPositionRange> for ChapterPositionRange {
    fn from(r: inkuna_core::ChapterPositionRange) -> Self {
        ChapterPositionRange {
            chapter_idx: r.chapter_idx,
            start_position: r.start_position,
            end_position: r.end_position,
        }
    }
}

/// The progress facade: per-page-turn position writes and range reports.
/// Constructed once by [`Bookshelf::open`], handed out by
/// `Bookshelf::progress()` as a cheap `Arc` clone.
#[derive(uniffi::Object)]
pub struct ShelfProgress(pub(crate) std::sync::Arc<inkuna_core::Library>);

#[uniffi::export(async_runtime = "tokio")]
impl ShelfProgress {
    /// One call per page turn. `coordinate` is the content coordinate of
    /// the page's first character; `progression` the book-wide total.
    /// Shells may pass `position: None` — the core derives the synthetic
    /// position from the coordinate.
    ///
    /// Pass `coordinate: None` when the caller has no engine coordinate:
    /// the stored coordinate is then left untouched (a book-start
    /// placeholder would destroy a rebaselined position irrecoverably)
    /// and only progression / recency / finished state are written.
    pub async fn update_progress(
        &self,
        id: String,
        coordinate: Option<Coordinate>,
        progression: f64,
        position: Option<u32>,
    ) -> Result<(), InkunaError> {
        let library = self.0.clone();
        blocking(move || {
            Ok(library.update_progress(&id, coordinate.map(Into::into), progression, position)?)
        })
        .await
    }

    /// The 1-based synthetic position of `coordinate` — session-free, so
    /// Home/Detail screens can label "page N of M" without opening a
    /// reader. A book with no position rows answers `1`; past-end
    /// coordinates clamp to the last position.
    pub async fn position_of(
        &self,
        id: String,
        coordinate: Coordinate,
    ) -> Result<u32, InkunaError> {
        let library = self.0.clone();
        blocking(move || Ok(library.position_of(&id, coordinate.into())?)).await
    }

    /// The publication's total synthetic position count, session-free. A
    /// book with no position rows answers `1`.
    pub async fn position_count(&self, id: String) -> Result<u32, InkunaError> {
        let library = self.0.clone();
        blocking(move || Ok(library.position_count(&id)?)).await
    }

    /// The coordinate at a 1-based synthetic position, session-free. Input
    /// below 1 clamps to the first position; input beyond the final position
    /// clamps to the last. The returned coordinate starts that position's
    /// 1024-character block, so it inverts `position_of` but not arbitrary
    /// coordinates within a block.
    pub async fn coordinate_at_position(
        &self,
        id: String,
        position: u32,
    ) -> Result<Coordinate, InkunaError> {
        let library = self.0.clone();
        blocking(move || Ok(library.coordinate_at_position(&id, position)?.into())).await
    }

    /// Every TOC entry's position span, in chapter order; empty until the
    /// book's synthetic positions are computed. Powers "pages left in
    /// this chapter" without opening the book.
    pub async fn chapter_position_ranges(
        &self,
        id: String,
    ) -> Result<Vec<ChapterPositionRange>, InkunaError> {
        let library = self.0.clone();
        blocking(move || {
            Ok(library
                .chapter_position_ranges(&id)?
                .into_iter()
                .map(Into::into)
                .collect())
        })
        .await
    }

    /// Explicit finish/unfinish; unfinishing sticks at end-of-book because
    /// auto-finish only fires on an upward crossing of the threshold.
    pub async fn set_finished(&self, id: String, finished: bool) -> Result<(), InkunaError> {
        let library = self.0.clone();
        blocking(move || Ok(library.set_finished(&id, finished)?)).await
    }
}
