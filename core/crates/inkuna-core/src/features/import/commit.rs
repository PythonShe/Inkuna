//! The commit stage: the parsed import in hand, put the book on disk and
//! its rows in the database.
//!
//! Ordering is the whole point here. The cover is written and the staged
//! book renamed into place *first*, and `books/` is flushed, before any row
//! is written — so a crash or a power loss leaves an unreferenced file
//! (which the open-time sweep collects) and never a row whose book is gone.

use super::budget::PersistBudget;
use super::model::ImportOutcome;
use super::pipeline::PreparedImport;
use super::restore;
use crate::core::files::sync_dir;
use crate::core::time::unix_now;
use crate::features::library::{join_authors, map_publication, Library, PUB_COLUMNS};
use crate::features::progress::synthetic_positions;
use crate::{CoreError, Format, Publication};

impl Library {
    /// Writes the cover, renames the staged book into place and flushes
    /// `books/` **first**, then writes all rows in one transaction — a
    /// crash in between leaves unreferenced files (swept at next open),
    /// never a fileless row. The directory flush is what makes that
    /// ordering hold across a power loss and not just a process crash. A
    /// concurrent import of the same content loses the unique-index race
    /// and resolves to `Duplicate`.
    pub(crate) fn commit_import(
        &self,
        prepared: PreparedImport,
    ) -> Result<ImportOutcome, CoreError> {
        // One meter per publication: a batch never shares it.
        self.commit_import_budgeted(prepared, PersistBudget::for_import())
    }

    /// [`commit_import`](Self::commit_import) with an explicit persistence
    /// meter; the production path always passes
    /// [`PersistBudget::for_import`], tests pass tiny ceilings to reach
    /// the trip path.
    pub(super) fn commit_import_budgeted(
        &self,
        mut prepared: PreparedImport,
        mut budget: PersistBudget,
    ) -> Result<ImportOutcome, CoreError> {
        // Taken out of `prepared` so the closures below can borrow it
        // independently of the fields the insert path moves.
        let restore = prepared.restore.take();
        // The guard that keeps a restore from silently jumping the reader
        // somewhere else: coordinates index the canonical corpus, so they
        // only survive if the corpus this import just built digests to
        // what the removal stamped. Different or unknown — degrade to
        // progression; the book still opens where it roughly was.
        let coordinates_restored = restore
            .as_ref()
            .is_some_and(|tombstone| restore::coordinates_survive(tombstone, &prepared.spine));
        let final_path = self.data_dir.join(&prepared.rel_path);

        let cover_rel = match &prepared.cover {
            Some(cover) => {
                let rel = format!("covers/{}.{}", prepared.id, cover.extension);
                let cover_path = self.data_dir.join(&rel);
                if let Err(e) = std::fs::write(&cover_path, &cover.bytes) {
                    // A write that fails part-way still leaves a partial
                    // file, and `cover_rel` never binds, so `cleanup_files`
                    // below could not reach it: sweep it here or it lingers
                    // unreferenced until the next Library::open. Safe even
                    // on a restore, where this path belongs to the adopted
                    // id: `fs::write` already truncated whatever was there,
                    // so removing the remnant cannot make it worse.
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

        // After the rename, `final_path` and the cover sit at the adopted
        // publication's own paths on a restore — paths a concurrent restore
        // of the same content may already have claimed with byte-identical
        // files. Deleting them could therefore strip a live row of its
        // book, so a failed restore leaves its files to the open-time
        // sweep (which is exactly what that sweep is for) instead.
        let cleanup_unless_restoring = |include_book: bool| {
            if restore.is_none() {
                cleanup_files(include_book);
            }
        };

        if let Err(e) = std::fs::rename(&prepared.tmp_path, &final_path) {
            let _ = std::fs::remove_file(&prepared.tmp_path);
            // The cover is already at the adopted id's path on a restore,
            // and a concurrent restore may have made that row live: going
            // through the restore-aware cleanup is what keeps this failure
            // from deleting a live book's only cover, which nothing heals
            // (the sweep collects only *unreferenced* files, and
            // `optimize_covers` just logs the read failure).
            cleanup_unless_restoring(false);
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
            cleanup_unless_restoring(true);
            return Err(e);
        }

        // Synthetic positions are a pure function of the canonical
        // projection (a textless resource counts 0 chars but still gets a
        // count-1 row — position math never has spine holes).
        let char_counts: Vec<u64> = prepared
            .spine
            .iter()
            .map(|(_, text)| {
                text.as_deref()
                    .map_or(0, |body| body.chars().count() as u64)
            })
            .collect();
        let position_rows = synthetic_positions(&char_counts);
        let position_total: u32 = position_rows
            .iter()
            .fold(0u32, |sum, &(_, _, count)| sum.saturating_add(count));

        let publication = Publication {
            id: prepared.id,
            title: prepared.title,
            authors: prepared.authors,
            language: prepared.language,
            text_encoding: prepared.text_encoding,
            format: Format::Epub,
            file_path: prepared.rel_path,
            cover_path: cover_rel.clone(),
            added_at: unix_now(),
            progression: 0.0,
            coordinate: None,
            position_count: Some(position_total),
            finished_at: None,
            last_opened_at: None,
        };
        let authors = join_authors(&publication.authors);

        // Each row is charged against the budget *before* its insert:
        // the transaction makes rollback free, but the WAL still grows on
        // disk while it runs, so the meter must abort within one row of
        // the ceiling rather than bound only the committed state.
        // Returns whether this import claimed the row; `false` is a lost
        // race, which the caller resolves to `Duplicate`.
        let insert =
            |tx: &rusqlite::Transaction, budget: &mut PersistBudget| -> Result<bool, CoreError> {
                budget.charge(
                    publication.title.len()
                        + authors.len()
                        + publication.language.as_deref().map_or(0, str::len)
                        + publication.text_encoding.as_deref().map_or(0, str::len),
                )?;
                if restore.is_some() {
                    if !restore::revive(tx, &publication, &authors, coordinates_restored)? {
                        return Ok(false);
                    }
                } else {
                    insert_publication(tx, &publication, &authors, &prepared.content_hash)?;
                }
                write_spine_rows(tx, budget, &prepared.spine, &publication.id)?;
                write_position_rows(tx, &position_rows, position_total, &publication.id)?;
                write_chapter_rows(tx, budget, &prepared.toc, &publication.id)?;
                Ok(true)
            };

        let claimed = {
            let mut conn = self.writer.lock().unwrap();
            // The book and cover are already on disk, so every early exit
            // from here on must sweep them or they linger unreferenced
            // until the next Library::open.
            let tx = match conn.transaction() {
                Ok(tx) => tx,
                Err(e) => {
                    cleanup_unless_restoring(true);
                    return Err(e.into());
                }
            };
            match insert(&tx, &mut budget) {
                Ok(true) => {
                    if let Err(e) = tx.commit() {
                        cleanup_unless_restoring(true);
                        return Err(e.into());
                    }
                    true
                }
                // A restore that found the tombstone already revived.
                Ok(false) => {
                    drop(tx);
                    false
                }
                Err(CoreError::Database(e)) if is_constraint_violation(&e) => {
                    drop(tx);
                    false
                }
                Err(e) => {
                    // Dropping the uncommitted transaction rolls every
                    // write back; budget trips take this path too.
                    drop(tx);
                    cleanup_unless_restoring(true);
                    return Err(e);
                }
            }
        };

        if claimed {
            // Outside the writer lock, from the texts already in hand.
            // Derived data: a failure only delays searchability of this
            // one book until the next open's reconcile pass.
            if let Err(e) = self.search.index_publication(
                &publication.id,
                prepared
                    .spine
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, (_, text))| text.as_deref().map(|body| (idx as u32, body))),
            ) {
                log::warn!("search indexing failed for {}: {e}", publication.id);
            }
            if restore.is_some() {
                // Re-read rather than report the locally built record: the
                // restored row carries the preserved progression, position,
                // finished state, and open time, none of which this import
                // computed.
                return Ok(ImportOutcome::Restored {
                    publication: self.publication(&publication.id)?,
                    coordinates_restored,
                });
            }
            Ok(ImportOutcome::Imported(publication))
        } else {
            // Lost the race: another import committed the same content
            // between our dedupe check and this write.
            cleanup_unless_restoring(true);
            match self.publication_by_hash(&prepared.content_hash)? {
                Some(existing) => Ok(ImportOutcome::Duplicate(existing)),
                None => Err(CoreError::NotFound(prepared.content_hash)),
            }
        }
    }

    /// The live publication holding this content, if any. Tombstones are
    /// excluded on purpose: this answers "is it already in the library",
    /// and a removed book is not.
    pub(super) fn publication_by_hash(&self, hash: &str) -> Result<Option<Publication>, CoreError> {
        self.readers.with(|conn| {
            let mut stmt = conn.prepare_cached(&format!(
                "SELECT {PUB_COLUMNS} FROM publications
                 WHERE content_hash = ?1 AND removed_at IS NULL"
            ))?;
            let mut rows = stmt.query_map([hash], map_publication)?;
            rows.next().transpose().map_err(Into::into)
        })
    }
}

/// A fresh book's row. Rebaselined by construction: its corpus IS the
/// canonical projection and its positions are computed alongside, so
/// `reconciled_at` is stamped now and the V8 reconcile pass skips the book.
/// The coordinate columns stay NULL — a fresh book has no reading position.
fn insert_publication(
    tx: &rusqlite::Transaction,
    publication: &Publication,
    authors: &str,
    content_hash: &str,
) -> Result<(), CoreError> {
    tx.execute(
        "INSERT INTO publications
            (id, title, authors, language, text_encoding, format, file_path,
             cover_path, content_hash, added_at, progression, reconciled_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        rusqlite::params![
            publication.id,
            publication.title,
            authors,
            publication.language,
            publication.text_encoding,
            publication.format.as_str(),
            publication.file_path,
            publication.cover_path,
            content_hash,
            publication.added_at,
            publication.progression,
            unix_now(),
        ],
    )?;
    Ok(())
}

/// One `resources` row per spine entry, plus the `resource_text` body for
/// every entry that yielded one. The spine position IS the `spine_idx` a
/// coordinate carries, so a textless resource still takes its slot.
fn write_spine_rows(
    tx: &rusqlite::Transaction,
    budget: &mut PersistBudget,
    spine: &[(String, Option<String>)],
    publication_id: &str,
) -> Result<(), CoreError> {
    for (spine_idx, (href, text)) in spine.iter().enumerate() {
        let resource_id = uuid::Uuid::new_v4().to_string();
        budget.charge(href.len())?;
        tx.execute(
            "INSERT INTO resources (id, publication_id, spine_idx, href)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![resource_id, publication_id, spine_idx as i64, href],
        )?;
        if let Some(body) = text {
            budget.charge(body.len())?;
            tx.execute(
                "INSERT INTO resource_text (resource_id, body) VALUES (?1, ?2)",
                rusqlite::params![resource_id, body.as_str()],
            )?;
        }
    }
    Ok(())
}

/// Positions land in the same transaction as the corpus, so "page N of M"
/// is real from the first open.
fn write_position_rows(
    tx: &rusqlite::Transaction,
    rows: &[(u32, u32, u32)],
    total: u32,
    publication_id: &str,
) -> Result<(), CoreError> {
    for &(spine_idx, start, count) in rows {
        tx.execute(
            "INSERT INTO resource_positions
                (publication_id, spine_idx, start_position, position_count)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![publication_id, spine_idx, start, count],
        )?;
    }
    tx.execute(
        "UPDATE publications SET position_count = ?1 WHERE id = ?2",
        rusqlite::params![total, publication_id],
    )?;
    Ok(())
}

fn write_chapter_rows(
    tx: &rusqlite::Transaction,
    budget: &mut PersistBudget,
    toc: &[crate::formats::epub::TocEntry],
    publication_id: &str,
) -> Result<(), CoreError> {
    for (idx, entry) in toc.iter().enumerate() {
        budget.charge(entry.title.len() + entry.href.len())?;
        tx.execute(
            "INSERT INTO chapters (id, publication_id, idx, title, href, depth)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                uuid::Uuid::new_v4().to_string(),
                publication_id,
                idx as i64,
                entry.title,
                entry.href,
                entry.depth,
            ],
        )?;
    }
    Ok(())
}

fn is_constraint_violation(e: &rusqlite::Error) -> bool {
    matches!(
        e,
        rusqlite::Error::SqliteFailure(err, _)
            if err.code == rusqlite::ErrorCode::ConstraintViolation
    )
}
