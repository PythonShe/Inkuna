//! EPUB parsing and writing for import. Parsing covers OPF metadata, the
//! spine (reading order), the flattened TOC (EPUB 3 nav doc with NCX
//! fallback), cover art, and per-resource plain text — the future search
//! corpus. The writer is the normalization target for other reflowable
//! formats. Rendering and pagination stay in the shells (Readium
//! navigators); the core parses once at import and never re-opens the book
//! afterwards.
//!
//! Hrefs are stored package-root-relative, percent-decoded, without a
//! leading slash (e.g. `OEBPS/ch01.xhtml`), TOC hrefs keeping their
//! fragment. Chapter→resource mapping is `href`-minus-fragment matched
//! against the spine — derivable at query time, never stored.

mod archive;
mod container;
mod cover;
mod href;
mod model;
mod opf;
mod package;
mod text;
mod toc;
mod write;
mod xml;

pub use model::{Cover, TocEntry};
pub use package::read_package;
pub use text::extract_spine_text;
pub(crate) use text::MAX_TOTAL_TEXT_BYTES;
#[allow(unused_imports)] // Consumed by the TXT converter in Step 2.
pub(crate) use write::EpubWriter;

#[cfg(test)]
pub(crate) use opf::{MAX_MANIFEST_ITEMS, MAX_METADATA_VALUE_BYTES};
#[cfg(test)]
pub(crate) use toc::{MAX_TOC_ENTRIES, MAX_TOC_TOTAL_BYTES};
