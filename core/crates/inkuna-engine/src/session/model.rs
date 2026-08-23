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
/// the session's lock — callbacks may query the session; calling
/// `close` from a callback is tolerated but skips the worker join).
///
/// This trait is a cross-plan contract mirrored 1:1 by the FFI: every
/// terminal outcome of laying a chapter out has exactly one event, so a
/// shell that starts in a loading state is always woken — success by
/// `first_page_ready`/`chapter_ready`, failure by `chapter_failed`.
/// Shells therefore never poll.
pub trait LayoutEvents: Send + Sync + 'static {
    /// Page 0 of `spine_idx` is available — the first-paint moment.
    fn first_page_ready(&self, generation: u64, spine_idx: u32);
    /// The chapter finished laying out with `page_count` pages.
    fn chapter_ready(&self, generation: u64, spine_idx: u32, page_count: u32);
    /// The chapter failed closed: unreadable, unparseable, or a layout
    /// panic. It carries NO page count and asserts NO readiness — the
    /// chapter has no queryable page and never will at this generation.
    /// From here every query on `spine_idx` returns its terminal error
    /// instead of `NotReady`, which is the shell's cue to render its
    /// unreadable-chapter placeholder in that slot.
    ///
    /// Required, deliberately: a listener that silently drops failures
    /// leaves a shell spinning forever, so that must be a compile error
    /// rather than an inherited no-op.
    fn chapter_failed(&self, generation: u64, spine_idx: u32);
}
