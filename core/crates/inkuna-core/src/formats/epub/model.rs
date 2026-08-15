//! The values one parse pass yields: metadata, TOC entries, cover art, and
//! the package that carries them into the import pipeline.

/// Dublin Core metadata off the OPF. Every value is capped at
/// `MAX_METADATA_VALUE_BYTES` on a `char` boundary and the creator list at
/// `MAX_AUTHORS`, so a crafted OPF cannot amplify here; a missing value is
/// `None`/empty rather than an error, and import derives a title from the
/// filename when the book carries none.
#[derive(Debug, Default)]
pub struct EpubMetadata {
    /// `dc:title`, absent in malformed books.
    pub title: Option<String>,
    /// `dc:creator` values in document order.
    pub authors: Vec<String>,
    /// `dc:language` as written (a BCP 47 tag in practice), unvalidated.
    pub language: Option<String>,
}

/// One entry of the flattened TOC (EPUB 3 nav doc, or NCX fallback), in
/// document order.
#[derive(Debug, Clone, PartialEq)]
pub struct TocEntry {
    pub title: String,
    /// Package-root-relative, possibly carrying a `#fragment` — the
    /// Readium jump target.
    pub href: String,
    /// 0-based nesting depth in the flattened tree.
    pub depth: u32,
}

/// Cover art read out of the archive, held in memory only between the
/// parse and the commit that writes it under `covers/`.
pub struct Cover {
    /// The image as stored in the archive; never re-encoded.
    pub bytes: Vec<u8>,
    /// Lowercase filename extension derived from the media type.
    pub extension: String,
}

/// Everything one pass over the archive yields for import. Each part
/// degrades independently: a book with no TOC, no cover, or no readable
/// spine still produces a package, because only the container, the OPF,
/// and a title are mandatory.
pub struct EpubPackage {
    pub metadata: EpubMetadata,
    /// Spine hrefs in reading order, resolved against the OPF's directory
    /// and deduplicated; entries whose resolved href exceeds
    /// `MAX_HREF_BYTES` are dropped rather than persisted.
    pub spine: Vec<String>,
    /// The flattened TOC, empty when the book has none or its nav/NCX
    /// could not be parsed.
    pub toc: Vec<TocEntry>,
    /// `None` when the book declares no cover, or its declared one is not
    /// a usable image.
    pub cover: Option<Cover>,
}
