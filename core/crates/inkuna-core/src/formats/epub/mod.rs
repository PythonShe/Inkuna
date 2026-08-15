//! EPUB parsing for import: OPF metadata, the spine (reading order), the
//! flattened TOC (EPUB 3 nav doc with NCX fallback), cover art, and
//! per-resource plain text — the future search corpus. Rendering and
//! pagination stay in the shells (Readium navigators); the core parses
//! once at import and never re-opens the book afterwards.
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
mod xml;

pub use model::{Cover, TocEntry};
pub use package::read_package;
pub use text::extract_spine_text;
