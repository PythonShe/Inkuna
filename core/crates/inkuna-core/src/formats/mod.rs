//! Publication formats: content-based detection plus one parser module per
//! format. Reflowable formats (EPUB, MOBI, AZW3, TXT) normalize to EPUB at
//! import; fixed-layout formats (PDF, CBZ/CBR) get dedicated navigators in
//! the shells, so nothing here ever renders.

pub(crate) mod epub;
pub(crate) mod format;

pub use format::Format;
