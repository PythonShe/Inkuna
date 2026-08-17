/// Every fallible operation in the core fails with one of these. The
/// variants are the distinctions a shell must be able to act on, which is
/// why a broken book (`InvalidPublication`), an unreadable file (`Io`), and
/// a damaged container (`Archive`) stay apart instead of collapsing into
/// one "cannot open" string: `inkuna-ffi` mirrors them one-for-one into
/// `InkunaError`, and the shells route their messaging off the variant.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// The filesystem failed: the source file is unreadable, storage is
    /// full, a device error hit mid-copy. Never a statement about the
    /// content of a book.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// The source is bigger than the import ceiling
    /// (`files::MAX_IMPORT_BYTES`), so it was refused mid-copy rather than
    /// allowed to fill the device — the guard that matters for a stream
    /// whose length nobody can know in advance. Carries the ceiling in
    /// bytes so a shell can name the limit it just hit.
    #[error("file too large: over {0} bytes")]
    FileTooLarge(u64),

    /// SQLite failed. Reaching a shell means the library database itself
    /// is in trouble; ordinary domain misses are `NotFound` instead.
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    /// The zip container is damaged or uses something unsupported (a
    /// compression method, a corrupt central directory) — as distinct from
    /// a well-formed archive whose EPUB structure is wrong, which is
    /// `InvalidPublication`.
    #[error("archive error: {0}")]
    Archive(String),

    /// Carries the detected format name when one was recognized but is not
    /// yet importable ("mobi"), so shells can say "MOBI support is coming"
    /// instead of a bare error; `None` means nothing recognizable at all.
    #[error("unsupported format{}", .0.as_deref().map(|f| format!(": {f}")).unwrap_or_default())]
    UnsupportedFormat(Option<String>),

    /// The file is the format it claims to be but cannot be imported as a
    /// publication: a mandatory part is missing, a required value (a title)
    /// cannot be derived, an entry blows a decompression cap, or the
    /// per-publication persistence budget tripped. The message names the
    /// specific failure for logs; it is not a user-facing string.
    #[error("invalid publication: {0}")]
    InvalidPublication(String),

    /// The tantivy search index failed — opening, writing, or querying
    /// it. The index is derived data: a shell can treat this as "search
    /// unavailable right now" and the next open rebuilds from the corpus.
    #[error("search index error: {0}")]
    Search(#[source] tantivy::TantivyError),

    /// No entity with that id — a publication, bookmark, or session the
    /// caller named that the library does not hold. Carries the id.
    #[error("publication not found: {0}")]
    NotFound(String),
}

impl From<zip::result::ZipError> for CoreError {
    fn from(e: zip::result::ZipError) -> Self {
        CoreError::Archive(e.to_string())
    }
}
