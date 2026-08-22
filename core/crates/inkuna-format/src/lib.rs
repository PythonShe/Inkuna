//! Import-side format conversion: DRM-free MOBI/AZW3 and plain-text
//! sources normalize to EPUB 3 through the shared writer. Consumed by
//! `inkuna-core`'s import pipeline; nothing here touches the DB or the
//! FFI. The `mobi` and `txt` modules are public so the pipeline's
//! `mobi::convert_to_epub` / `txt::convert_to_epub` call shape survives
//! the move unchanged.

pub(crate) mod azw3;
mod epub_write;
mod error;
pub mod mobi;
pub mod txt;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

pub use epub_write::EpubWriter;
pub use error::FormatError;
