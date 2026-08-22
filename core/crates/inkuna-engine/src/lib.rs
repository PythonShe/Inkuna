//! The reader layout engine: parse → style → shape → break → paginate →
//! display lists. Deterministic fixed-point layout; no DB access; archive
//! reads via inkuna-content only.

pub mod display;
pub mod dom;
mod error;
pub mod fixed;
pub mod fonts;
pub mod layout;
pub mod paginate;
pub mod session;
pub mod settings;
pub mod shape;
pub mod style;
pub mod text;

#[cfg(test)]
mod test_support;

pub use display::{
    build_page, digest_hex, page_digest, A11yBlock, A11yRole, ColorRole, Decoration,
    DecorationKind, DisplayContext, GlyphRun, ImagePlacement, LineGeom, LinkRegion,
    PageDisplayList, PageMaps, Rect,
};
pub use dom::{parse, Document};
pub use error::EngineError;
pub use fixed::Fx;
pub use fonts::{FontAxis, FontEntry, FontRegistry, LoadedFace};
pub use layout::{
    break_paragraph, Line, LineOptions, PositionedRun, SegmentKind, ShapedParagraph,
    ShapedSegment, MAX_LINES_PER_PARAGRAPH,
};
pub use paginate::{
    paginate, ChapterInput, ChapterLayoutResult, DecorationKind as PlacedDecorationKind, FxRect,
    FxSize, LaidPage, PlacedDecoration, PlacedImage, PlacedLine, MAX_PAGES_PER_CHAPTER,
};
pub use session::{
    ChapterGeometry, CharRange, EngineSession, HitResult, LayoutEvents, PageLocation,
    SelectionRect, Viewport,
};
pub use settings::{FontFamily, LayoutSettings, Typography};
pub use shape::{
    itemize, shape_ruby, shape_text, Glyph, Item, RubyRun, RunOrientation, RunStyle, ShapeContext,
    ShapedRun,
};
pub use style::{
    cap_sheet_sources, parse_sheet, resolve, ComputedStyle, FontStyle, FontWeight, RubyPosition,
    StyledDocument, WritingMode,
};
pub use text::{project, Coordinate, Projection, TextSpan};
