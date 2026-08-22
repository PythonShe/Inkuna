//! The reader-session FFI surface: content coordinates, the display and
//! session record mirrors, the layout-event listener, and the
//! `ReaderSession` object itself.

mod listener;
mod records;
mod session;

pub use listener::LayoutListener;
pub(crate) use listener::ListenerAdapter;
pub use records::{
    A11yBlock, A11yRole, ChapterGeometry, CharRange, ColorRole, Coordinate, Decoration,
    DecorationKind, FontAxis, FontEntry, GlyphRun, HitResult, ImagePlacement, LinkRegion,
    PageDisplayList, PageLocation, ReaderLayoutSettings, Rect, RunOrientation, SelectionRect,
    Viewport, WritingMode,
};
pub use session::ReaderSession;
