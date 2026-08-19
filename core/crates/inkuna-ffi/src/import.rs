//! Import calls: path- and descriptor-based, single and batch, with
//! progress observation.

use std::sync::Arc;

use crate::bookshelf::{blocking, Bookshelf};
use crate::error::InkunaError;
use crate::library::{publication_record, Publication};

#[derive(Debug, uniffi::Enum)]
pub enum ImportOutcome {
    Imported {
        publication: Publication,
    },
    /// The library already holds this content; nothing was added.
    Duplicate {
        publication: Publication,
    },
    /// Batch-only: one bad file never aborts the rest of a selection.
    /// Carries the same typed error the single-file path throws, so the
    /// two paths classify failures identically.
    Failed {
        path: String,
        error: InkunaError,
    },
}

/// One open file descriptor to import. Android's SAF hands out streams,
/// not paths; passing the descriptor itself lets the core's copy into its
/// own storage be the only copy, with no shell-side staging layer.
#[derive(Debug, uniffi::Record)]
pub struct FdImport {
    /// An open, readable file descriptor. **Ownership transfers with the
    /// call**: the shell must detach it first (Android's
    /// `ParcelFileDescriptor.detachFd()`), and the core reads and closes
    /// it exactly once, success or failure. It need not be seekable — a
    /// provider pipe works.
    pub fd: i32,
    /// The provider's name for the document, verbatim (CJK included). It
    /// stands in for the filename everywhere one is needed: the TXT
    /// extension check, the fallback title, failure reporting, and
    /// progress events.
    pub display_name: String,
}

/// Observes a batch import while it runs. Implemented by the shells;
/// called from Rust worker threads, so implementations must hop to their
/// own main thread before touching UI.
#[uniffi::export(with_foreign)]
pub trait ImportProgressListener: Send + Sync {
    /// One file finished — imported, duplicate, or failed. `completed`
    /// counts finished files including this one and is strictly
    /// increasing across calls; `path` names the input that finished,
    /// which is not necessarily the batch's next one, because files
    /// finish in parallel.
    fn on_file_complete(&self, completed: u32, total: u32, path: String);
}

/// Maps a core batch outcome into the FFI record, absolutizing paths.
fn batch_record(
    library: &inkuna_core::Library,
    outcome: inkuna_core::BatchImportOutcome,
) -> ImportOutcome {
    match outcome {
        inkuna_core::BatchImportOutcome::Imported(p) => ImportOutcome::Imported {
            publication: publication_record(library, p),
        },
        inkuna_core::BatchImportOutcome::Duplicate(p) => ImportOutcome::Duplicate {
            publication: publication_record(library, p),
        },
        inkuna_core::BatchImportOutcome::Failed { path, error } => ImportOutcome::Failed {
            path,
            error: error.into(),
        },
    }
}

/// Adopts a descriptor the shell detached for us.
///
/// SAFETY: `FdImport`'s contract is that the shell hands over sole
/// ownership; from here the `File` reads and closes it exactly once.
fn file_from(fd: i32) -> std::fs::File {
    use std::os::fd::FromRawFd;
    unsafe { std::fs::File::from_raw_fd(fd) }
}

#[uniffi::export(async_runtime = "tokio")]
impl Bookshelf {
    /// Imports one file: copied into core-owned storage, hashed for
    /// dedupe, parsed for metadata/TOC/cover/text. Never returns `Failed`
    /// — hard I/O failures throw instead.
    pub async fn import(&self, path: String) -> Result<ImportOutcome, InkunaError> {
        let library = self.0.clone();
        blocking(move || {
            Ok(match library.import(&path)? {
                inkuna_core::ImportOutcome::Imported(p) => ImportOutcome::Imported {
                    publication: publication_record(&library, p),
                },
                inkuna_core::ImportOutcome::Duplicate(p) => ImportOutcome::Duplicate {
                    publication: publication_record(&library, p),
                },
            })
        })
        .await
    }

    /// Imports many files (document pickers multi-select), parallelizing
    /// the parse stage; per-item failures come back as `Failed` items in
    /// input order instead of throwing. `listener`, when given, hears one
    /// event per finished file while the call runs.
    pub async fn import_batch(
        &self,
        paths: Vec<String>,
        listener: Option<Arc<dyn ImportProgressListener>>,
    ) -> Result<Vec<ImportOutcome>, InkunaError> {
        let library = self.0.clone();
        blocking(move || {
            let total = paths.len() as u32;
            Ok(library
                .import_batch_with(&paths, &|done, path| {
                    if let Some(listener) = &listener {
                        listener.on_file_complete(done as u32, total, path.to_string());
                    }
                })
                .into_iter()
                .map(|outcome| batch_record(&library, outcome))
                .collect())
        })
        .await
    }

    /// [`import`](Self::import) over an open file descriptor — see
    /// [`FdImport`] for the ownership contract. Never returns `Failed`;
    /// hard failures throw instead.
    pub async fn import_fd(&self, item: FdImport) -> Result<ImportOutcome, InkunaError> {
        let library = self.0.clone();
        blocking(move || {
            let mut file = file_from(item.fd);
            Ok(
                match library.import_reader(&mut file, &item.display_name)? {
                    inkuna_core::ImportOutcome::Imported(p) => ImportOutcome::Imported {
                        publication: publication_record(&library, p),
                    },
                    inkuna_core::ImportOutcome::Duplicate(p) => ImportOutcome::Duplicate {
                        publication: publication_record(&library, p),
                    },
                },
            )
        })
        .await
    }

    /// [`import_batch`](Self::import_batch) over open file descriptors.
    /// Outcomes come back in input order; `Failed.path` and progress
    /// events carry each item's `display_name`, the only name a stream
    /// has. Every descriptor is closed by this call, whatever happens.
    pub async fn import_batch_fds(
        &self,
        items: Vec<FdImport>,
        listener: Option<Arc<dyn ImportProgressListener>>,
    ) -> Result<Vec<ImportOutcome>, InkunaError> {
        let library = self.0.clone();
        blocking(move || {
            let total = items.len() as u32;
            // Adopt every descriptor up front, before any work can fail,
            // so each one is closed exactly once no matter what follows.
            let items: Vec<(std::fs::File, String)> = items
                .into_iter()
                .map(|item| (file_from(item.fd), item.display_name))
                .collect();
            Ok(library
                .import_batch_readers(items, &|done, name| {
                    if let Some(listener) = &listener {
                        listener.on_file_complete(done as u32, total, name.to_string());
                    }
                })
                .into_iter()
                .map(|outcome| batch_record(&library, outcome))
                .collect())
        })
        .await
    }

    /// Re-encodes covers persisted before import-time normalization
    /// existed into the bounded WebP form, returning how many changed.
    /// Idempotent and cheap when there is nothing to do — fire it once
    /// in the background after startup, never on the critical path.
    pub async fn optimize_covers(&self) -> Result<u32, InkunaError> {
        let library = self.0.clone();
        blocking(move || Ok(library.optimize_covers()?)).await
    }
}
