//! Publication formats: content-based detection stays here; parsing lives
//! in `inkuna-content` and conversion in `inkuna-format`, re-exported so
//! the import pipeline's `epub::` / `mobi::` / `txt::` call shape is
//! unchanged. Reflowable formats (EPUB, MOBI, AZW3, TXT) normalize to
//! EPUB at import; fixed-layout formats (PDF, CBZ/CBR) get dedicated
//! navigators.

pub(crate) mod epub;
pub(crate) mod format;

pub(crate) use inkuna_format::{mobi, txt};

pub use format::Format;
