//! Reading-progress writes and the chapter position ranges derived from
//! the core-computed synthetic positions.

use crate::bookshelf::blocking;
use crate::error::InkunaError;
use crate::reader::Coordinate;

/// One TOC entry's span of synthetic positions; 1-based, both bounds
/// inclusive. Sparse per chapter: entries whose href matches no spine
/// resource are absent.
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
    pub async fn update_progress(
        &self,
        id: String,
        coordinate: Coordinate,
        progression: f64,
        position: Option<u32>,
    ) -> Result<(), InkunaError> {
        let library = self.0.clone();
        blocking(move || {
            Ok(library.update_progress(&id, coordinate.into(), progression, position)?)
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
