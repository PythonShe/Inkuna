//! Vertical business slices. Each folder owns one user-facing capability
//! end to end — types, queries, and writes together — and contributes its
//! own `impl Library` block.

pub(crate) mod import;
pub(crate) mod library;
pub(crate) mod progress;
pub(crate) mod search;
pub(crate) mod settings;
pub(crate) mod stats;
