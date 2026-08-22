//! EPUB container access and parsing: the archive layer (budget-bounded
//! entry reads), `META-INF/container.xml`, the OPF package document, the
//! flattened TOC (EPUB 3 nav doc with NCX fallback), cover art, hrefs, and
//! per-resource plain text. The container layer serves both import and the
//! core-owned reader engine, which re-opens archives at read time.
//!
//! Hrefs are stored package-root-relative, percent-decoded, without a
//! leading slash (e.g. `OEBPS/ch01.xhtml`), TOC hrefs keeping their
//! fragment.

mod archive;
mod container;
mod cover;
mod error;
mod href;
mod model;
mod opf;
mod package;
mod text;
mod toc;
mod xml;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

pub use archive::{read_resource, MAX_SPINE_ENTRY_BYTES};
pub use error::ContentError;
pub use href::{resolve_href, resolve_relative, split_fragment};
pub use model::{Cover, EpubMetadata, EpubPackage, TocEntry};
pub use opf::{MAX_MANIFEST_ITEMS, MAX_METADATA_VALUE_BYTES, MAX_SPINE_ITEMS};
pub use package::read_package;
pub use text::{extract_spine_text, MAX_TOTAL_TEXT_BYTES};
pub use toc::{MAX_TOC_ENTRIES, MAX_TOC_TOTAL_BYTES};
