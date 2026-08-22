//! The exported reader session: synchronous cache-only queries (safe on
//! the UI thread — they throw instead of blocking) plus the two heavy
//! async entry points.

use std::sync::Arc;

use crate::bookshelf::blocking;
use crate::error::InkunaError;

use super::records::{
    A11yBlock, ChapterGeometry, CharRange, Coordinate, FontEntry, HitResult, PageDisplayList,
    PageLocation, ReaderLayoutSettings, SelectionRect, Viewport,
};

/// Maps an engine failure across the boundary through the core taxonomy.
fn engine_err(e: inkuna_core::EngineError) -> InkunaError {
    InkunaError::from(inkuna_core::CoreError::from(e))
}

/// One open publication with a background layout worker. Every sync
/// method is cache-only and non-blocking: a miss schedules the chapter
/// and throws `NotReady` — a shell re-queries after a layout event.
/// Position lookups run over a snapshot of the publication's synthetic
/// position ranges loaded once at open — sync-safe, no DB access.
#[derive(uniffi::Object)]
pub struct ReaderSession {
    pub(crate) session: Arc<inkuna_core::EngineSession>,
    /// The registry the session shapes with, for `font_registry()`.
    pub(crate) fonts: Arc<inkuna_core::FontRegistry>,
    /// `(spine_idx, start_position, position_count)` rows, spine order.
    pub(crate) ranges: Vec<(u32, u32, u32)>,
}

#[uniffi::export]
impl ReaderSession {
    /// The chapter's geometry. Requires the complete chapter.
    pub fn chapter(&self, spine_idx: u32) -> Result<ChapterGeometry, InkunaError> {
        self.session
            .chapter(spine_idx)
            .map(Into::into)
            .map_err(engine_err)
    }

    /// One page's display list. Progressive: succeeds as soon as THAT
    /// page is published.
    pub fn page(&self, spine_idx: u32, page_idx: u32) -> Result<PageDisplayList, InkunaError> {
        self.session
            .page(spine_idx, page_idx)
            .map(Into::into)
            .map_err(engine_err)
    }

    /// Whether the chapter is completely laid out at the current
    /// generation. Pure read — never schedules.
    pub fn is_ready(&self, spine_idx: u32) -> bool {
        self.session.is_ready(spine_idx)
    }

    /// The page holding a coordinate; offsets at or past the laid
    /// prefix clamp to the last page.
    pub fn locate(&self, coordinate: Coordinate) -> Result<PageLocation, InkunaError> {
        self.session
            .locate(coordinate.into())
            .map(Into::into)
            .map_err(engine_err)
    }

    /// Resolves a TOC/link href to a coordinate. Fragment-free lookups
    /// resolve from the spine model alone and never wait.
    pub fn locate_href(
        &self,
        href: String,
        fragment: Option<String>,
    ) -> Result<Coordinate, InkunaError> {
        self.session
            .locate_href(&href, fragment.as_deref())
            .map(Into::into)
            .map_err(engine_err)
    }

    /// The coordinate (and link target, if any) under a page point.
    pub fn hit_test(
        &self,
        spine_idx: u32,
        page_idx: u32,
        x: f64,
        y: f64,
    ) -> Result<HitResult, InkunaError> {
        self.session
            .hit_test(spine_idx, page_idx, x, y)
            .map(Into::into)
            .map_err(engine_err)
    }

    /// Selection rects for a char range, page-local (callers clamp
    /// ranges to one page).
    pub fn selection_rects(
        &self,
        spine_idx: u32,
        range: CharRange,
    ) -> Result<Vec<SelectionRect>, InkunaError> {
        self.session
            .selection_rects(spine_idx, range.into())
            .map(|rects| rects.into_iter().map(Into::into).collect())
            .map_err(engine_err)
    }

    /// The word containing a coordinate.
    pub fn word_at(&self, coordinate: Coordinate) -> Result<CharRange, InkunaError> {
        self.session
            .word_at(coordinate.into())
            .map(Into::into)
            .map_err(engine_err)
    }

    /// The canonical projection text of a char range.
    pub fn text_range(&self, spine_idx: u32, range: CharRange) -> Result<String, InkunaError> {
        self.session
            .text_range(spine_idx, range.into())
            .map_err(engine_err)
    }

    /// Highlight rects for a search match, page-local like
    /// `selection_rects`.
    pub fn match_rects(
        &self,
        spine_idx: u32,
        char_offset: u64,
        len: u64,
    ) -> Result<Vec<SelectionRect>, InkunaError> {
        self.session
            .match_rects(spine_idx, char_offset, len)
            .map(|rects| rects.into_iter().map(Into::into).collect())
            .map_err(engine_err)
    }

    /// The page's accessible blocks, in logical reading order.
    pub fn accessibility_blocks(
        &self,
        spine_idx: u32,
        page_idx: u32,
    ) -> Result<Vec<A11yBlock>, InkunaError> {
        self.session
            .accessibility_blocks(spine_idx, page_idx)
            .map(|blocks| blocks.into_iter().map(Into::into).collect())
            .map_err(engine_err)
    }

    /// The registry faces the session shapes with; shells rebuild
    /// platform fonts from these entries.
    pub fn font_registry(&self) -> Vec<FontEntry> {
        self.fonts.entries().into_iter().map(Into::into).collect()
    }

    /// Number of spine resources.
    pub fn spine_count(&self) -> u32 {
        self.session.spine_len()
    }

    /// The page's canonical char range.
    pub fn page_char_range(&self, spine_idx: u32, page_idx: u32) -> Result<CharRange, InkunaError> {
        self.session
            .page_char_range(spine_idx, page_idx)
            .map(Into::into)
            .map_err(engine_err)
    }

    /// The coordinate's 1-based synthetic position, from the snapshot
    /// loaded at open — the shells never mirror the 1024-char constant.
    /// A coordinate past the snapshot clamps to the last position; a
    /// book with no rows answers 1.
    pub fn position_of(&self, coordinate: Coordinate) -> u32 {
        inkuna_core::position_for(&self.ranges, coordinate.into()).unwrap_or(1)
    }

    /// The publication's total synthetic position count, from the same
    /// snapshot; a book with no rows answers 1.
    pub fn position_count(&self) -> u32 {
        self.ranges
            .last()
            .map(|&(_, start, count)| start.saturating_add(count.saturating_sub(1)))
            .unwrap_or(1)
    }

    /// The page's canonical digest (layout-determinism fingerprint).
    pub fn page_digest(&self, spine_idx: u32, page_idx: u32) -> Result<String, InkunaError> {
        self.session
            .page_digest(spine_idx, page_idx)
            .map_err(engine_err)
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl ReaderSession {
    /// Relayout under a new viewport/settings: bumps the generation,
    /// invalidates the cache, and schedules the current chapter first.
    pub async fn update_layout(
        &self,
        viewport: Viewport,
        settings: ReaderLayoutSettings,
    ) -> Result<(), InkunaError> {
        let session = self.session.clone();
        blocking(move || {
            session.update_layout(viewport.into(), settings.into());
            Ok(())
        })
        .await
    }

    /// One resource's bytes (images the display lists reference),
    /// budget-capped by the container layer.
    pub async fn resource(&self, href: String) -> Result<Vec<u8>, InkunaError> {
        let session = self.session.clone();
        blocking(move || session.resource(&href).map_err(engine_err)).await
    }
}
