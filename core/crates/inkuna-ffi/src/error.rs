//! The one error type every exported method throws.

/// Mirrors `CoreError` variant-for-variant, *structured*: no
/// `flat_error`, so the shells receive fields, not a Display string to
/// parse. `format` in `UnsupportedFormat` survives the boundary as the
/// value the core detected, and nothing downstream ever needs to
/// prefix-match an error message. The field is `detail` rather than
/// `message` because the generated Kotlin exception classes would
/// otherwise collide with `Throwable.message`.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum InkunaError {
    #[error("io error: {detail}")]
    Io { detail: String },
    #[error("file too large: over {limit} bytes")]
    FileTooLarge { limit: u64 },
    #[error("database error: {detail}")]
    Database { detail: String },
    #[error("archive error: {detail}")]
    Archive { detail: String },
    #[error("unsupported format{}", format.as_deref().map(|f| format!(": {f}")).unwrap_or_default())]
    UnsupportedFormat { format: Option<String> },
    #[error("invalid publication: {detail}")]
    InvalidPublication { detail: String },
    /// The reader engine has not laid out the requested page or position
    /// yet — re-query after a layout event, never an error about the book.
    #[error("layout not ready")]
    NotReady { detail: String },
    /// Content beyond what the reader engine can lay out (fixed-layout,
    /// an unsupported structure). `detail` is for logs, not users.
    #[error("unsupported content: {detail}")]
    UnsupportedContent { detail: String },
    #[error("layout budget exceeded: {detail}")]
    LayoutBudgetExceeded { detail: String },
    /// A jump target (TOC fragment, stored position) that does not exist
    /// in the laid-out content.
    #[error("anchor not found: {detail}")]
    AnchorNotFound { detail: String },
    /// The search index failed; it is derived data, so a shell can treat
    /// this as "search unavailable right now".
    #[error("search index error: {detail}")]
    Search { detail: String },
    #[error("publication not found: {id}")]
    NotFound { id: String },
}

impl From<inkuna_core::CoreError> for InkunaError {
    fn from(e: inkuna_core::CoreError) -> Self {
        use inkuna_core::CoreError as C;
        match e {
            C::Io(e) => InkunaError::Io {
                detail: e.to_string(),
            },
            C::FileTooLarge(limit) => InkunaError::FileTooLarge { limit },
            C::Database(e) => InkunaError::Database {
                detail: e.to_string(),
            },
            C::Archive(m) => InkunaError::Archive { detail: m },
            C::UnsupportedFormat(f) => InkunaError::UnsupportedFormat { format: f },
            C::InvalidPublication(m) => InkunaError::InvalidPublication { detail: m },
            C::NotReady => InkunaError::NotReady {
                detail: String::new(),
            },
            C::UnsupportedContent(m) => InkunaError::UnsupportedContent { detail: m },
            C::LayoutBudgetExceeded(m) => InkunaError::LayoutBudgetExceeded { detail: m },
            C::AnchorNotFound(m) => InkunaError::AnchorNotFound { detail: m },
            C::Search(e) => InkunaError::Search {
                detail: e.to_string(),
            },
            C::NotFound(id) => InkunaError::NotFound { id },
        }
    }
}
