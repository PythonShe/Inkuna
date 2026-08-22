//! Opinionated style resolution: a tiny honored-property cascade over the
//! arena DOM. Publisher CSS steers structure (visibility, direction,
//! writing mode) and emphasis; the reader's settings own the visual
//! typography downstream. The whole pass is ignore-what-you-don't-know by
//! design and infallible.

mod cascade;
mod model;
mod sheet;

#[cfg(test)]
mod tests;

pub use cascade::resolve;
pub use model::{
    ComputedStyle, Direction, FontStyle, FontWeight, RubyPosition, StyledDocument, TextAlign,
    WritingMode,
};
pub use sheet::{cap_sheet_sources, parse_sheet, Stylesheet};
