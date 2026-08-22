//! The session's public value types and the layout-event callback
//! trait. The FFI mirrors these 1:1 in M6.

use crate::display::Rect;
use crate::style::WritingMode;
use crate::text::Coordinate;

/// The page frame in layout points at 1×.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub width: f64,
    pub height: f64,
}

/// A canonical char range, `end` exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CharRange {
    pub start: u64,
    pub end: u64,
}

/// One chapter's laid-out geometry.
///
/// `truncated` means the resource hit a parse/layout budget and
/// rendered only its laid-out prefix (shells show a notice).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChapterGeometry {
    pub generation: u64,
    pub page_count: u32,
    pub char_range: CharRange,
    pub writing_mode: WritingMode,
    /// The package-level `page-progression-direction="rtl"` flag; a
    /// session-layer fact — geometry never encodes it.
    pub rtl_progression: bool,
    pub truncated: bool,
}

/// A located page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageLocation {
    pub generation: u64,
    pub spine_idx: u32,
    pub page_idx: u32,
}

/// A hit-test result: the resolved coordinate plus the link target when
/// the point lands inside a link region.
#[derive(Debug, Clone, PartialEq)]
pub struct HitResult {
    pub coordinate: Coordinate,
    pub link_target: Option<String>,
}

/// One selection rect with the writing mode the shell needs to draw
/// its handles correctly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SelectionRect {
    pub rect: Rect,
    pub writing_mode: WritingMode,
}

/// Layout progress callbacks, fired from the worker thread (never under
/// the session's lock — callbacks may query the session, but must not
/// call `close`).
pub trait LayoutEvents: Send + Sync + 'static {
    /// Page 0 of `spine_idx` is available — the first-paint moment.
    fn first_page_ready(&self, generation: u64, spine_idx: u32);
    /// The chapter finished laying out with `page_count` pages.
    fn chapter_ready(&self, generation: u64, spine_idx: u32, page_count: u32);
}
