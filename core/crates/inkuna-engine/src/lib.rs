//! The reader layout engine: parse → style → shape → break → paginate →
//! display lists. Deterministic fixed-point layout; no DB access; archive
//! reads via inkuna-content only.

mod error;

#[cfg(test)]
mod test_support;

pub use error::EngineError;
