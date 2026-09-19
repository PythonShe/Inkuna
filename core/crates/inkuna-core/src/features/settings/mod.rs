//! App settings: a single core-owned row with core-owned defaults, read
//! and written whole. `reading_theme` is an opaque identifier — shells own
//! the palettes, and unknown ids are stored as-is so themes can ship
//! shell-first. New fields arrive by migration.

mod model;
mod store;

#[cfg(test)]
mod tests;

pub use model::Settings;
/// The current default reading-size step, applied to a library created
/// fresh — see `core::db::migrate::seed_fresh_defaults`.
pub(crate) use model::DEFAULT_TEXT_SIZE_STEP;
