//! Line breaking and justification: shaped paragraphs → positioned
//! lines. Break opportunities come from UAX-14 (`unicode-linebreak`),
//! which encodes CJK kinsoku; bidi runs are reordered to visual order
//! (UBA L2); justification distributes deficits in exact 1/64 units —
//! never a float. Everything here is `Fx`.

mod assemble;
mod justify;
mod lines;

#[cfg(test)]
mod tests;

pub use lines::{
    break_paragraph, Line, LineOptions, PositionedRun, SegmentKind, ShapedParagraph,
    ShapedSegment, MAX_LINES_PER_PARAGRAPH,
};
