//! Text shaping: bidi + script itemization, rustybuzz shaping with the
//! reading→CJK→symbols→`.notdef` fallback chain, vertical-mode
//! orientation, and ruby runs.

mod itemize;
mod ruby;
mod shape;
mod vertical;

#[cfg(test)]
mod tests;

pub use itemize::{itemize, Item};
pub use ruby::{shape_ruby, RubyRun};
pub use shape::{shape_text, Glyph, RunOrientation, RunStyle, ShapeContext, ShapedRun};
