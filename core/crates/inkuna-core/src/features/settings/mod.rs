//! App settings: a single core-owned row with core-owned defaults, read
//! and written whole. `reading_theme` is an opaque identifier — shells own
//! the palettes, and unknown ids are stored as-is so themes can ship
//! shell-first. New fields arrive by migration.

mod model;
mod store;

#[cfg(test)]
mod tests;

pub use model::Settings;
