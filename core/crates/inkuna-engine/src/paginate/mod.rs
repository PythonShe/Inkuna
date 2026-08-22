//! Block flow and progressive pagination: the styled document's blocks
//! (paragraphs, headings, blockquotes, pre, lists, degraded tables,
//! rules, images) laid into pages emitted as they fill. Fixed-point
//! throughout; the float boundary is `Metrics::new`.

mod blocks;
mod flow;
mod images;
mod pages;
mod shaping;

#[cfg(test)]
mod tests;

pub use images::PlacedImage;
pub use pages::{
    paginate, ChapterInput, ChapterLayoutResult, DecorationKind, FxRect, FxSize, LaidPage,
    PlacedDecoration, PlacedLine, MAX_PAGES_PER_CHAPTER,
};
