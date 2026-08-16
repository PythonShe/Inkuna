//! The stages themselves: stage and hash, dedupe, parse, commit.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::budget::PersistBudget;
use super::model::{BatchImportOutcome, ImportOutcome};
use crate::core::files::{copy_and_hash, sync_dir};
use crate::core::time::unix_now;
use crate::features::library::{join_authors, map_publication, Library, PUB_COLUMNS};
use crate::formats::epub;
use crate::{CoreError, Format, Publication};

/// A fully parsed import, ready to commit: the file already sits at
/// `tmp_path` and every DB value is computed. Parsing happens outside any
/// lock; committing takes the writer.
pub(crate) struct PreparedImport {
    id: String,
    tmp_path: PathBuf,
    rel_path: String,
    content_hash: String,
    title: String,
    authors: Vec<String>,
    language: Option<String>,
    /// Spine hrefs in reading order, paired with each resource's extracted
    /// plain text (`None` = malformed or budget-skipped resource, its text
    /// row is skipped). Repeated spine entries share one extraction.
    spine: Vec<(String, Option<Arc<str>>)>,
    toc: Vec<epub::TocEntry>,
    cover: Option<epub::Cover>,
}

pub(crate) enum Prepared {
    Duplicate(Box<Publication>),
    Fresh(Box<PreparedImport>),
}

impl Library {
    /// Imports one file at `path`: detect the format, copy it into
    /// core-owned storage while hashing (BLAKE3), dedupe on that hash,
    /// parse the copy for metadata, spine, TOC, cover, and text, then
    /// commit. The source file is never modified or moved.
    ///
    /// A crafted or damaged book degrades rather than failing wherever the
    /// missing part is optional — an oversized TOC is truncated, an
    /// unreadable chapter loses only its text row, an unusable cover is
    /// dropped. It fails with `UnsupportedFormat` for anything but EPUB
    /// today, and with `InvalidPublication` when a mandatory part is
    /// missing, no title can be derived, or the per-publication
    /// persistence budget trips (in which case the transaction rolls back
    /// and the staged file is swept).
    pub fn import(&self, path: &str) -> Result<ImportOutcome, CoreError> {
        match self.prepare_import(path)? {
            Prepared::Duplicate(existing) => Ok(ImportOutcome::Duplicate(*existing)),
            Prepared::Fresh(prepared) => self.commit_import(*prepared),
        }
    }

    /// Imports many files, parallelizing the copy/hash/parse stage with
    /// rayon; DB commits serialize per-item on the writer, which is fine
    /// because parsing dominates. Reuses the single-import pipeline
    /// verbatim; outcomes come back in input order. Two identical files in
    /// one batch resolve to Imported + Duplicate via the unique-index race.
    pub fn import_batch(&self, paths: &[String]) -> Vec<BatchImportOutcome> {
        self.import_batch_with(paths, &|_, _| {})
    }

    /// [`import_batch`](Self::import_batch) reporting progress: `on_done`
    /// fires once per finished file with the count of files done so far
    /// (including that one) and the input path that finished. Counts are
    /// strictly increasing across calls, but the paths need not arrive in
    /// input order — files finish in parallel. Called from rayon worker
    /// threads, under an internal lock that keeps the counts ordered, so
    /// keep the callback quick.
    pub fn import_batch_with(
        &self,
        paths: &[String],
        on_done: &(dyn Fn(usize, &str) + Sync),
    ) -> Vec<BatchImportOutcome> {
        use rayon::prelude::*;
        let done = std::sync::Mutex::new(0usize);
        paths
            .par_iter()
            .map(|path| {
                let outcome = match self.import(path) {
                    Ok(ImportOutcome::Imported(p)) => BatchImportOutcome::Imported(p),
                    Ok(ImportOutcome::Duplicate(p)) => BatchImportOutcome::Duplicate(p),
                    Err(error) => BatchImportOutcome::Failed {
                        path: path.clone(),
                        error,
                    },
                };
                {
                    // The callback runs under the lock so a consumer never
                    // sees the counter go backwards.
                    let mut done = done.lock().unwrap();
                    *done += 1;
                    on_done(*done, path);
                }
                outcome
            })
            .collect()
    }

    /// Streams the source into a `.tmp` under `books/` while hashing,
    /// checks the hash against the library, and parses the copy. No writer
    /// lock is held at any point.
    pub(crate) fn prepare_import(&self, path: &str) -> Result<Prepared, CoreError> {
        let src = Path::new(path);
        let format = Format::detect(src)?;
        if format != Format::Epub {
            return Err(CoreError::UnsupportedFormat(Some(format.as_str().to_string())));
        }

        let id = uuid::Uuid::new_v4().to_string();
        let rel_path = format!("books/{id}.epub");
        let tmp_path = self.data_dir.join(format!("books/{id}.epub.tmp"));

        let content_hash = copy_and_hash(src, &tmp_path).inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp_path);
        })?;

        if let Some(existing) = self.publication_by_hash(&content_hash)? {
            let _ = std::fs::remove_file(&tmp_path);
            return Ok(Prepared::Duplicate(Box::new(existing)));
        }

        let parsed = epub::read_package(&tmp_path).inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp_path);
        })?;
        let title = parsed
            .metadata
            .title
            .or_else(|| src.file_stem().map(|s| nfc(&s.to_string_lossy())))
            .ok_or_else(|| {
                let _ = std::fs::remove_file(&tmp_path);
                CoreError::InvalidPublication("untitled".into())
            })?;

        // Rayon across resources: the corpus keys off the spine, so it is
        // complete even for books with no TOC.
        let texts = epub::extract_spine_text(&tmp_path, &parsed.spine);
        let spine = parsed.spine.into_iter().zip(texts).collect();

        Ok(Prepared::Fresh(Box::new(PreparedImport {
            id,
            tmp_path,
            rel_path,
            content_hash,
            title,
            authors: parsed.metadata.authors,
            language: parsed.metadata.language,
            spine,
            toc: parsed.toc,
            cover: parsed.cover,
        })))
    }

    /// Writes the cover, renames the staged book into place and flushes
    /// `books/` **first**, then inserts all rows in one transaction — a
    /// crash in between leaves unreferenced files (swept at next open),
    /// never a fileless row. The directory flush is what makes that
    /// ordering hold across a power loss and not just a process crash. A
    /// concurrent import of the same content loses the unique-index race
    /// and resolves to `Duplicate`.
    pub(crate) fn commit_import(&self, prepared: PreparedImport) -> Result<ImportOutcome, CoreError> {
        // One meter per publication: a batch never shares it.
        self.commit_import_budgeted(prepared, PersistBudget::for_import())
    }

    /// [`commit_import`](Self::commit_import) with an explicit persistence
    /// meter; the production path always passes
    /// [`PersistBudget::for_import`], tests pass tiny ceilings to reach
    /// the trip path.
    pub(super) fn commit_import_budgeted(
        &self,
        prepared: PreparedImport,
        mut budget: PersistBudget,
    ) -> Result<ImportOutcome, CoreError> {
        let final_path = self.data_dir.join(&prepared.rel_path);

        let cover_rel = match &prepared.cover {
            Some(cover) => {
                let rel = format!("covers/{}.{}", prepared.id, cover.extension);
                let cover_path = self.data_dir.join(&rel);
                if let Err(e) = std::fs::write(&cover_path, &cover.bytes) {
                    // A write that fails part-way still leaves a partial
                    // file, and `cover_rel` never binds, so `cleanup_files`
                    // below could not reach it: sweep it here or it lingers
                    // unreferenced until the next Library::open.
                    let _ = std::fs::remove_file(&cover_path);
                    let _ = std::fs::remove_file(&prepared.tmp_path);
                    return Err(e.into());
                }
                Some(rel)
            }
            None => None,
        };
        let cleanup_files = |include_book: bool| {
            if include_book {
                let _ = std::fs::remove_file(&final_path);
            }
            if let Some(rel) = &cover_rel {
                let _ = std::fs::remove_file(self.data_dir.join(rel));
            }
        };

        if let Err(e) = std::fs::rename(&prepared.tmp_path, &final_path) {
            let _ = std::fs::remove_file(&prepared.tmp_path);
            cleanup_files(false);
            return Err(e.into());
        }
        // `copy_and_hash` fsynced the bytes, but the rename that names them
        // only lives in the directory cache, so the file-before-row
        // ordering above is not durable until `books/` is flushed. Doing it
        // here — before the commit — is what keeps a power loss from
        // leaving a row whose book is gone; the reverse (an unreferenced
        // file) is swept at the next open. One extra directory fsync per
        // import is nothing beside the whole-file copy, hash, and parse the
        // import already paid, and the book is the irreplaceable artifact.
        // `covers/` deliberately gets no such flush: a cover is derived
        // data, re-creatable from the book we just made durable.
        if let Err(e) = sync_dir(&self.data_dir.join("books")) {
            cleanup_files(true);
            return Err(e);
        }

        let publication = Publication {
            id: prepared.id,
            title: prepared.title,
            authors: prepared.authors,
            language: prepared.language,
            format: Format::Epub,
            file_path: prepared.rel_path,
            cover_path: cover_rel.clone(),
            added_at: unix_now(),
            progression: 0.0,
            locator: None,
            position_count: None,
            finished_at: None,
            last_opened_at: None,
        };

        // Each row is charged against the budget *before* its insert:
        // the transaction makes rollback free, but the WAL still grows on
        // disk while it runs, so the meter must abort within one row of
        // the ceiling rather than bound only the committed state.
        let insert = |tx: &rusqlite::Transaction,
                      budget: &mut PersistBudget|
         -> Result<(), CoreError> {
            let authors = join_authors(&publication.authors);
            budget.charge(
                publication.title.len()
                    + authors.len()
                    + publication.language.as_deref().map_or(0, str::len),
            )?;
            tx.execute(
                "INSERT INTO publications
                    (id, title, authors, language, format, file_path, cover_path,
                     content_hash, added_at, progression)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                rusqlite::params![
                    publication.id,
                    publication.title,
                    authors,
                    publication.language,
                    publication.format.as_str(),
                    publication.file_path,
                    publication.cover_path,
                    prepared.content_hash,
                    publication.added_at,
                    publication.progression,
                ],
            )?;
            for (spine_idx, (href, text)) in prepared.spine.iter().enumerate() {
                let resource_id = uuid::Uuid::new_v4().to_string();
                budget.charge(href.len())?;
                tx.execute(
                    "INSERT INTO resources (id, publication_id, spine_idx, href)
                     VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![resource_id, publication.id, spine_idx as i64, href],
                )?;
                if let Some(body) = text {
                    budget.charge(body.len())?;
                    tx.execute(
                        "INSERT INTO resource_text (resource_id, body) VALUES (?1, ?2)",
                        rusqlite::params![resource_id, body.as_ref()],
                    )?;
                }
            }
            for (idx, entry) in prepared.toc.iter().enumerate() {
                budget.charge(entry.title.len() + entry.href.len())?;
                tx.execute(
                    "INSERT INTO chapters (id, publication_id, idx, title, href, depth)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    rusqlite::params![
                        uuid::Uuid::new_v4().to_string(),
                        publication.id,
                        idx as i64,
                        entry.title,
                        entry.href,
                        entry.depth,
                    ],
                )?;
            }
            Ok(())
        };

        let inserted = {
            let mut conn = self.writer.lock().unwrap();
            // The book and cover are already on disk, so every early exit
            // from here on must sweep them or they linger unreferenced
            // until the next Library::open.
            let tx = match conn.transaction() {
                Ok(tx) => tx,
                Err(e) => {
                    cleanup_files(true);
                    return Err(e.into());
                }
            };
            match insert(&tx, &mut budget) {
                Ok(()) => {
                    if let Err(e) = tx.commit() {
                        cleanup_files(true);
                        return Err(e.into());
                    }
                    true
                }
                Err(CoreError::Database(e)) if is_constraint_violation(&e) => {
                    drop(tx);
                    false
                }
                Err(e) => {
                    // Dropping the uncommitted transaction rolls every
                    // insert back; budget trips take this path too.
                    drop(tx);
                    cleanup_files(true);
                    return Err(e);
                }
            }
        };

        if inserted {
            Ok(ImportOutcome::Imported(publication))
        } else {
            // Lost the race: another import committed the same content
            // between our dedupe check and this insert.
            cleanup_files(true);
            match self.publication_by_hash(&prepared.content_hash)? {
                Some(existing) => Ok(ImportOutcome::Duplicate(existing)),
                None => Err(CoreError::NotFound(prepared.content_hash)),
            }
        }
    }

    fn publication_by_hash(&self, hash: &str) -> Result<Option<Publication>, CoreError> {
        self.readers.with(|conn| {
            let mut stmt = conn.prepare_cached(&format!(
                "SELECT {PUB_COLUMNS} FROM publications WHERE content_hash = ?1"
            ))?;
            let mut rows = stmt.query_map([hash], map_publication)?;
            rows.next().transpose().map_err(Into::into)
        })
    }
}

/// Normalizes a filename-derived title to NFC. File providers hand back
/// decomposed forms on some volumes (HFS+/APFS most of all), and a
/// decomposed CJK or Hangul title renders identically but compares, sorts,
/// and searches differently from every composed string in the library.
/// Titles from inside a book are left verbatim — this is only for names the
/// filesystem made up. Living here, no shell can forget it.
fn nfc(name: &str) -> String {
    icu_normalizer::ComposingNormalizer::new_nfc()
        .normalize(name)
        .into_owned()
}

fn is_constraint_violation(e: &rusqlite::Error) -> bool {
    matches!(
        e,
        rusqlite::Error::SqliteFailure(err, _)
            if err.code == rusqlite::ErrorCode::ConstraintViolation
    )
}
