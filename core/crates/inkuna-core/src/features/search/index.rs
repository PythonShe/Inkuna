//! The persistent tantivy index under `<data_dir>/index/`: one document
//! per spine resource, built from `resource_text` — never by re-parsing
//! books. Import and remove keep it current; a background reconcile pass
//! at every open heals whatever a crash or an older database left behind,
//! and an unopenable index is wiped and rebuilt from the same corpus, so
//! the index is always disposable and the database stays the truth.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use tantivy::collector::Count;
use tantivy::query::TermQuery;
use tantivy::schema::{
    Field, IndexRecordOption, Schema, TextFieldIndexing, TextOptions, STORED, STRING,
};
use tantivy::tokenizer::TextAnalyzer;
use tantivy::{doc, Index, IndexReader, IndexWriter, ReloadPolicy, Searcher, TantivyDocument};

use super::tokenize::{CjkUnigramTokenizer, WordTokenizer};
use crate::core::db::open_connection;
use crate::CoreError;

/// One writer thread with a modest arena: imports are the only writers
/// and they serialize on the mutex anyway, and phones prefer memory over
/// indexing throughput.
const WRITER_MEMORY_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Copy)]
pub(super) struct Fields {
    pub(super) publication_id: Field,
    pub(super) spine_idx: Field,
    pub(super) body_word: Field,
    pub(super) body_uni: Field,
}

pub(crate) struct SearchIndex {
    index: Index,
    writer: Arc<Mutex<IndexWriter>>,
    reader: IndexReader,
    fields: Fields,
    reconcile: Mutex<Option<JoinHandle<()>>>,
    /// Set by `Drop` before joining the reconcile thread, so the V8
    /// rebaseline and the reconcile body bail promptly instead of a
    /// `Library` drop blocking on a full-library re-projection.
    cancel: Arc<AtomicBool>,
}

fn schema() -> (Schema, Fields) {
    let mut builder = Schema::builder();
    let text = |tokenizer: &str| {
        TextOptions::default().set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer(tokenizer)
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        )
    };
    let fields = Fields {
        publication_id: builder.add_text_field("publication_id", STRING | STORED),
        spine_idx: builder.add_u64_field("spine_idx", STORED),
        body_word: builder.add_text_field("body_word", text("ink_word")),
        body_uni: builder.add_text_field("body_uni", text("ink_uni")),
    };
    (builder.build(), fields)
}

fn register_tokenizers(index: &Index) {
    index
        .tokenizers()
        .register("ink_word", TextAnalyzer::builder(WordTokenizer).build());
    index.tokenizers().register(
        "ink_uni",
        TextAnalyzer::builder(CjkUnigramTokenizer).build(),
    );
}

fn open_index(dir: &Path) -> Result<Index, CoreError> {
    std::fs::create_dir_all(dir)?;
    let mmap = tantivy::directory::MmapDirectory::open(dir)
        .map_err(|e| CoreError::Search(tantivy::TantivyError::OpenDirectoryError(e)))?;
    Index::open_or_create(mmap, schema().0).map_err(|e| CoreError::Search(e))
}

impl SearchIndex {
    /// Opens (creating if needed) the index at `<data_dir>/index/`. An
    /// index that cannot be opened — corrupt, or an incompatible older
    /// schema — is deleted and recreated empty; the reconcile pass then
    /// re-fills it from `resource_text`.
    pub(crate) fn open(data_dir: &Path) -> Result<SearchIndex, CoreError> {
        let dir = data_dir.join("index");
        let index = match open_index(&dir) {
            Ok(index) => index,
            Err(_) => {
                std::fs::remove_dir_all(&dir)?;
                open_index(&dir)?
            }
        };
        register_tokenizers(&index);
        let (_, fields) = schema();
        let writer = index
            .writer_with_num_threads(1, WRITER_MEMORY_BYTES)
            .map_err(|e| CoreError::Search(e))?;
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()
            .map_err(|e: tantivy::TantivyError| CoreError::Search(e))?;
        Ok(SearchIndex {
            index,
            writer: Arc::new(Mutex::new(writer)),
            reader,
            fields,
            reconcile: Mutex::new(None),
            cancel: Arc::new(AtomicBool::new(false)),
        })
    }

    pub(super) fn fields(&self) -> Fields {
        self.fields
    }

    /// A searcher over everything committed so far. Manual reload keeps
    /// freshness deterministic: a search issued after an import commit
    /// always sees that import, and nothing pays for a watcher thread.
    pub(super) fn searcher(&self) -> Result<Searcher, CoreError> {
        self.reader.reload().map_err(|e| CoreError::Search(e))?;
        Ok(self.reader.searcher())
    }

    /// (Re-)indexes one publication from its extracted texts: one doc per
    /// text-bearing spine resource. Replaces any previous docs for the
    /// id, so it is safe for both import and reconcile to call.
    pub(crate) fn index_publication<'a>(
        &self,
        publication_id: &str,
        resources: impl IntoIterator<Item = (u32, &'a str)>,
    ) -> Result<(), CoreError> {
        self.write_handle()
            .index_publication(publication_id, resources)
    }

    /// A cloneable, thread-independent write handle for background passes
    /// (the V8 rebaseline) that re-index publications book-by-book while
    /// they run.
    pub(crate) fn write_handle(&self) -> IndexWriteHandle {
        IndexWriteHandle {
            writer: Arc::clone(&self.writer),
            fields: self.fields,
        }
    }

    /// Drops every doc of one publication.
    pub(crate) fn delete_publication(&self, publication_id: &str) -> Result<(), CoreError> {
        let writer = self.writer.lock().unwrap();
        writer.delete_term(tantivy::Term::from_field_text(
            self.fields.publication_id,
            publication_id,
        ));
        commit(writer)
    }

    /// Compares the index against the database in a background thread and
    /// heals both directions: indexes publications missing from the
    /// index, drops docs whose publication is gone. The thread owns its
    /// own read connection, so an open returns immediately even when a
    /// large library needs a full rebuild.
    ///
    /// `pre` runs first on the spawned thread, opening its own resources
    /// — the V8 rebaseline chains here so the reconcile body never
    /// indexes a corpus the rebaseline is about to replace. It receives
    /// the index's drop-time cancellation flag and must poll it at its
    /// own safe points; the reconcile body is skipped entirely once the
    /// flag is set, so a dropped `Library` joins promptly.
    pub(crate) fn spawn_reconcile(
        &self,
        db_path: PathBuf,
        pre: impl FnOnce(&AtomicBool) + Send + 'static,
    ) {
        let index = self.index.clone();
        let writer = Arc::clone(&self.writer);
        let fields = self.fields;
        let cancel = Arc::clone(&self.cancel);
        let handle = std::thread::spawn(move || {
            pre(&cancel);
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            if let Err(e) = reconcile(&index, &writer, fields, &db_path) {
                log::warn!("search index reconcile failed: {e}");
            }
        });
        *self.reconcile.lock().unwrap() = Some(handle);
    }

    /// Blocks until a spawned reconcile pass has finished; used by tests
    /// and harmless when none is running.
    #[cfg(test)]
    pub(crate) fn wait_for_reconcile(&self) {
        if let Some(handle) = self.reconcile.lock().unwrap().take() {
            let _ = handle.join();
        }
    }
}

/// A live reconcile thread holds an `Arc` clone of the writer, which in
/// turn holds tantivy's on-disk lockfile — so dropping the index without
/// joining the thread leaves the lock held and a subsequent open on the
/// same directory fails with `LockBusy`. The join must not wait out the
/// whole pass, though: the thread runs the entire V8 rebaseline before
/// the reconcile body, so the cancel flag is set first and the pass
/// bails at its next check, leaving unstamped books to the next open.
impl Drop for SearchIndex {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(handle) = self.reconcile.lock().unwrap().take() {
            let _ = handle.join();
        }
    }
}

/// A write handle detached from the [`SearchIndex`]'s lifetime concerns:
/// it shares the writer mutex and schema fields, so a background pass can
/// replace one publication's docs at a time. Holding one keeps tantivy's
/// on-disk lockfile alive exactly as holding the index does — the pass
/// runs on the reconcile thread, which `Drop` joins.
#[derive(Clone)]
pub(crate) struct IndexWriteHandle {
    writer: Arc<Mutex<IndexWriter>>,
    fields: Fields,
}

impl IndexWriteHandle {
    /// (Re-)indexes one publication: `delete_term` + re-add, the same
    /// replace path [`SearchIndex::index_publication`] takes.
    pub(crate) fn index_publication<'a>(
        &self,
        publication_id: &str,
        resources: impl IntoIterator<Item = (u32, &'a str)>,
    ) -> Result<(), CoreError> {
        let mut writer = self.writer.lock().unwrap();
        if let Err(e) = add_publication(&writer, self.fields, publication_id, resources) {
            let _ = writer.rollback();
            return Err(e);
        }
        commit(writer)
    }
}

fn add_publication<'a>(
    writer: &IndexWriter,
    fields: Fields,
    publication_id: &str,
    resources: impl IntoIterator<Item = (u32, &'a str)>,
) -> Result<(), CoreError> {
    writer.delete_term(tantivy::Term::from_field_text(
        fields.publication_id,
        publication_id,
    ));
    for (spine_idx, body) in resources {
        writer
            .add_document(doc!(
                fields.publication_id => publication_id,
                fields.spine_idx => u64::from(spine_idx),
                fields.body_word => body,
                fields.body_uni => body,
            ))
            .map_err(|e| CoreError::Search(e))?;
    }
    Ok(())
}

fn commit(mut writer: std::sync::MutexGuard<'_, IndexWriter>) -> Result<(), CoreError> {
    match writer.commit() {
        Ok(_) => Ok(()),
        Err(e) => {
            let _ = writer.rollback();
            Err(CoreError::Search(e))
        }
    }
}

fn reconcile(
    index: &Index,
    writer: &Mutex<IndexWriter>,
    fields: Fields,
    db_path: &Path,
) -> Result<(), CoreError> {
    let conn = open_connection(db_path)?;
    let db_ids: HashSet<String> = {
        let mut stmt = conn.prepare("SELECT id FROM publications")?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        rows.collect::<Result<_, _>>()?
    };

    // Terms of deleted-but-unmerged docs may linger; deleting them again
    // is a no-op, and the missing set is computed from live terms only.
    let reader: IndexReader = index
        .reader_builder()
        .reload_policy(ReloadPolicy::Manual)
        .try_into()
        .map_err(|e: tantivy::TantivyError| CoreError::Search(e))?;
    let searcher = reader.searcher();
    let mut indexed = HashSet::new();
    for segment in searcher.segment_readers() {
        let inverted = segment
            .inverted_index(fields.publication_id)
            .map_err(|e| CoreError::Search(e))?;
        let mut terms = inverted
            .terms()
            .stream()
            .map_err(|e| CoreError::Search(e.into()))?;
        while terms.advance() {
            let term = tantivy::Term::from_field_bytes(fields.publication_id, terms.key());
            let live_docs = searcher
                .search(&TermQuery::new(term, IndexRecordOption::Basic), &Count)
                .map_err(CoreError::Search)?;
            if live_docs > 0 {
                indexed.insert(String::from_utf8_lossy(terms.key()).into_owned());
            }
        }
    }

    let stale: Vec<&String> = indexed.difference(&db_ids).collect();
    let missing: Vec<&String> = db_ids.difference(&indexed).collect();
    if stale.is_empty() && missing.is_empty() {
        return Ok(());
    }

    // Read all missing resources before taking the writer lock. Reconcile
    // must not block an import while SQLite is being read.
    let missing_resources: Vec<(&String, Vec<(u32, String)>)> = {
        let mut stmt = conn.prepare(
            "SELECT r.spine_idx, t.body FROM resources r
             JOIN resource_text t ON t.resource_id = r.id
             WHERE r.publication_id = ?1 ORDER BY r.spine_idx",
        )?;
        missing
            .into_iter()
            .map(|id| {
                let resources = stmt
                    .query_map([id], |row| Ok((row.get(0)?, row.get(1)?)))?
                    .collect::<Result<_, rusqlite::Error>>()?;
                Ok((id, resources))
            })
            .collect::<Result<_, rusqlite::Error>>()?
    };

    let writer = writer.lock().unwrap();
    for id in stale {
        writer.delete_term(tantivy::Term::from_field_text(fields.publication_id, id));
    }
    for (id, resources) in missing_resources {
        add_publication(
            &writer,
            fields,
            id,
            resources.iter().map(|(idx, body)| (*idx, body.as_str())),
        )?;
    }
    commit(writer)
}

// The doc values a search reads back out of a hit.
pub(super) fn doc_fields(
    searcher: &Searcher,
    fields: Fields,
    address: tantivy::DocAddress,
) -> Result<(String, u32), CoreError> {
    use tantivy::schema::Value;
    let doc: TantivyDocument = searcher.doc(address).map_err(|e| CoreError::Search(e))?;
    let publication_id = doc
        .get_first(fields.publication_id)
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let spine_idx = doc
        .get_first(fields.spine_idx)
        .and_then(|v| v.as_u64())
        .unwrap_or_default() as u32;
    Ok((publication_id, spine_idx))
}
