//! Reading progress: one write per page turn, core-computed synthetic
//! positions over the canonical projection, and finish semantics.
//! `progression` is always the book-wide total (0.0..=1.0), never
//! per-resource.

mod model;
mod positions;
mod writes;

#[cfg(test)]
mod tests;

pub use model::ChapterPositionRange;
pub(crate) use positions::synthetic_positions;
