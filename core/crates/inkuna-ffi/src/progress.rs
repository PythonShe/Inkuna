//! Reading-progress writes and the chapter position ranges derived from
//! the core-computed synthetic positions.

use crate::bookshelf::blocking;
use crate::error::InkunaError;

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
    /// One call per page turn. `locator` is the opaque Readium locator
    /// JSON; `progression` the book-wide totalProgression; `position` the
    /// synthetic position, once the navigator knows it.
    pub async fn update_progress(
        &self,
        id: String,
        locator: String,
        progression: f64,
        position: Option<u32>,
    ) -> Result<(), InkunaError> {
        let library = self.0.clone();
        blocking(move || Ok(library.update_progress(&id, &locator, progression, position)?)).await
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
