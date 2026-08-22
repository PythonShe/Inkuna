//! The reader layout engine: parse → style → shape → break → paginate →
//! display lists. Deterministic fixed-point layout; no DB access; archive
//! reads via inkuna-content only.

pub mod dom;
mod error;
pub mod fixed;
pub mod fonts;
pub mod settings;
pub mod shape;
pub mod style;
pub mod text;

#[cfg(test)]
mod test_support;

pub use dom::{parse, Document};
pub use error::EngineError;
pub use fixed::Fx;
pub use fonts::{FaceRole, FontAxis, FontEntry, FontRegistry, LoadedFace};
pub use settings::{FontFamily, LayoutSettings, Typography};
pub use shape::{shape_text, Glyph, RunOrientation, RunStyle, ShapeContext, ShapedRun};
pub use style::{
    cap_sheet_sources, parse_sheet, resolve, ComputedStyle, StyledDocument, WritingMode,
};
pub use text::{project, Coordinate, Projection, TextSpan};
