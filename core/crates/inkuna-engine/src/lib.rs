//! The reader layout engine: parse → style → shape → break → paginate →
//! display lists. Deterministic fixed-point layout; no DB access; archive
//! reads via inkuna-content only.

pub mod dom;
mod error;
pub mod settings;

#[cfg(test)]
mod test_support;

pub use dom::{parse, Document};
pub use error::EngineError;
pub use settings::{FontFamily, LayoutSettings, Typography};
