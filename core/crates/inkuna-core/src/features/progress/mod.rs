//! Reading progress: one write per page turn, Readium synthetic positions,
//! and finish semantics. `progression` is always the book-wide
//! `totalProgression` (0.0..=1.0), never per-resource; the locator blob is
//! opaque — stored and returned, never parsed.

mod model;
mod positions;
mod writes;

#[cfg(test)]
mod tests;

pub use model::ChapterPositionRange;
