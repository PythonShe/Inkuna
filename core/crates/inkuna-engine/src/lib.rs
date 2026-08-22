//! The reader layout engine: parse → style → shape → break → paginate →
//! display lists. Deterministic fixed-point layout; no DB access; archive
//! reads via inkuna-content only.

pub mod dom;
mod error;

#[cfg(test)]
mod test_support;

pub use dom::{parse, Document};
pub use error::EngineError;
