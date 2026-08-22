//! The canonical text stream and the coordinate system built on it.
//!
//! INVARIANT: the projection depends only on publisher CSS + UA defaults —
//! never on reader settings — so search corpus, positions, locators,
//! layout, and migration all index one identical stream.

mod coordinate;
mod projection;

pub use coordinate::Coordinate;
pub use projection::{project, Projection, TextSpan};
