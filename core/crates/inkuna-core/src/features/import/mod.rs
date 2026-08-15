//! Import pipeline: one streaming pass copies the source into core-owned
//! storage while hashing it (BLAKE3), dedupes on the hash, parses the copy,
//! then commits atomically — rename first, DB transaction second, so a
//! crash leaves an unreferenced file (swept at next open), never a fileless
//! row. Invariant: a committed row always points at an existing file.

mod model;
mod pipeline;

pub use model::{BatchImportOutcome, ImportOutcome};
