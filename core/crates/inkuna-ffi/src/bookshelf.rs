//! The exported library object and the plumbing every method shares.

use std::sync::Arc;

use crate::error::InkunaError;

/// The one UniFFI object; named `Bookshelf` because UniFFI's Kotlin output
/// imports JNA's `com.sun.jna.Library` and `Library` would collide.
///
/// Methods are async on a tokio runtime: SQLite and archive I/O run on
/// blocking threads while the shells get idiomatic Swift `await` / Kotlin
/// `suspend` — never a blocked main thread. Each feature module contributes
/// its own `impl Bookshelf` block.
#[derive(uniffi::Object)]
pub struct Bookshelf(pub(crate) Arc<inkuna_core::Library>);

#[uniffi::export]
impl Bookshelf {
    /// `data_dir` is the core-owned storage root (Application Support /
    /// `filesDir`): the DB, imported books, and covers all live under it.
    ///
    /// Hold exactly one `Bookshelf` per `data_dir` for the process lifetime:
    /// opening sweeps files no row references, so a second concurrent
    /// instance on the same directory deletes the first one's in-flight
    /// import.
    #[uniffi::constructor]
    pub fn open(data_dir: String) -> Result<Arc<Self>, InkunaError> {
        Ok(Arc::new(Bookshelf(Arc::new(inkuna_core::Library::open(&data_dir)?))))
    }
}

/// Runs sync core work on tokio's blocking pool, the bridge every async
/// method goes through.
pub(crate) async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, InkunaError> + Send + 'static,
) -> Result<T, InkunaError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| InkunaError::Io { detail: format!("task join error: {e}") })?
}

#[uniffi::export]
pub fn core_version() -> String {
    inkuna_core::version().to_string()
}
