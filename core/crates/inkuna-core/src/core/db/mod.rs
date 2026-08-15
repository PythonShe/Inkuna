//! SQLite plumbing: shared connection setup, the fixed reader pool, and the
//! append-only schema migrations.

pub(crate) mod migrate;
pub(crate) mod pool;

#[cfg(test)]
mod tests;

pub(crate) use migrate::migrate;
pub(crate) use pool::{open_connection, ReaderPool, READER_POOL_SIZE};
