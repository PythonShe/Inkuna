//! EPUB parsing and writing for import. Parsing — OPF metadata, the spine
//! (reading order), the flattened TOC (EPUB 3 nav doc with NCX fallback),
//! cover art, and per-resource plain text (the search corpus) — lives in
//! the `inkuna-content` crate and is re-exported here for the import
//! pipeline; the container layer serves both import and the core-owned
//! reader engine, which re-opens archives at read time. The writer is the
//! normalization target for other reflowable formats.
//!
//! Hrefs are stored package-root-relative, percent-decoded, without a
//! leading slash (e.g. `OEBPS/ch01.xhtml`), TOC hrefs keeping their
//! fragment. Chapter→resource mapping is `href`-minus-fragment matched
//! against the spine — derivable at query time, never stored.

mod write;

pub use inkuna_content::{extract_spine_text, read_package, Cover, TocEntry};
pub(crate) use inkuna_content::MAX_TOTAL_TEXT_BYTES;
#[allow(unused_imports)] // Consumed by the TXT converter in Step 2.
pub(crate) use write::EpubWriter;

#[cfg(test)]
pub(crate) use inkuna_content::{
    MAX_MANIFEST_ITEMS, MAX_METADATA_VALUE_BYTES, MAX_TOC_ENTRIES, MAX_TOC_TOTAL_BYTES,
};
