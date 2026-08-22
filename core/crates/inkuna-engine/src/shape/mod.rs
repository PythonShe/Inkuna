//! Text shaping: bidi + script itemization, rustybuzz shaping with the
//! reading→CJK→symbols→`.notdef` fallback chain, vertical-mode
//! orientation, and ruby runs.

mod itemize;
mod shape;

#[cfg(test)]
mod tests;

pub use itemize::{itemize, Item};
pub use shape::{Glyph, RunOrientation, RunStyle, ShapeContext, ShapedRun, shape_text};
