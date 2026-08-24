//! The engine's error type — final shape now, so later movements only
//! add call sites, never variants.

/// Every fallible operation in the engine fails with one of these.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    /// The publication opened, but this content is beyond what the
    /// engine can lay out (an unsupported document structure, a resource
    /// kind it cannot shape). The detail names the specific failure for
    /// logs; it is not a user-facing string.
    #[error("unsupported content: {detail}")]
    UnsupportedContent { detail: String },

    /// A layout budget tripped — the bounded-resource discipline the
    /// container layer enforces on reads, applied to what layout builds
    /// from them.
    #[error("layout budget exceeded: {detail}")]
    BudgetExceeded { detail: String },

    /// The caller asked for a page or position before the layout pass
    /// that would produce it has run.
    #[error("layout not ready")]
    NotReady,

    /// A jump target (a TOC fragment, a stored position) that does not
    /// exist in the laid-out content.
    #[error("anchor not found: {detail}")]
    AnchorNotFound { detail: String },

    /// The filesystem failed underneath a read. Never a statement about
    /// the content of a book.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// The container layer failed: the archive is damaged, an entry blew
    /// a decompression cap, the publication is malformed.
    #[error(transparent)]
    Content(#[from] inkuna_content::ContentError),
}
