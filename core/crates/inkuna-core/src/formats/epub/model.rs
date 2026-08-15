//! The values one parse pass yields: metadata, TOC entries, cover art, and
//! the package that carries them into the import pipeline.

#[derive(Debug, Default)]
pub struct EpubMetadata {
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub language: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TocEntry {
    pub title: String,
    /// Package-root-relative, possibly carrying a `#fragment` — the
    /// Readium jump target.
    pub href: String,
    /// 0-based nesting depth in the flattened tree.
    pub depth: u32,
}

pub struct Cover {
    pub bytes: Vec<u8>,
    /// Lowercase filename extension derived from the media type.
    pub extension: String,
}

pub struct EpubPackage {
    pub metadata: EpubMetadata,
    /// Spine hrefs in reading order.
    pub spine: Vec<String>,
    pub toc: Vec<TocEntry>,
    pub cover: Option<Cover>,
}
