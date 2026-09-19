//! The commit stage: the parsed import in hand, put the book on disk and
//! its rows in the database.
//!
//! Ordering is the whole point here, and it is not the same on both paths.
//!
//! A **fresh** import owns a brand-new UUID, so nothing else in the process
//! can even name its destination paths. It writes the cover, renames the
//! staged book into place and flushes `books/` *first*, before any row is
//! written — so a crash or a power loss leaves an unreferenced file (which
//! the open-time sweep collects) and never a row whose book is gone.
//!
//! A **restore** adopts a *removed* book's id, which makes its destination
//! paths shared property: a concurrent restore of the same content targets
//! them too, and `Library::remove` deletes them. Placing files there before
//! claiming the row is what lets a remove unlink a restore's book, or a
//! second restore truncate a live row's cover — neither of which anything
//! heals, since the sweep collects only *unreferenced* files. So a restore
//! stages both files under names no other actor can spell and renames them
//! into place under the writer lock, after `revive` reported the tombstone
//! claimed and before the transaction commits. A failure there rolls the
//! rows back; a restore that loses the revive race renames nothing and
//! drops its temps, leaving the winner's files untouched.

use super::budget::PersistBudget;
use super::model::ImportOutcome;
use super::pipeline::PreparedImport;
use super::restore::{self, HashMatch};
use crate::core::files::sync_dir;
use crate::core::time::unix_now;
use crate::features::library::{join_authors, title_key, Library};
use crate::features::progress::synthetic_positions;
use crate::{CoreError, Format, Publication};

/// How many times one `commit_import` call may start over as a restore
/// after losing the content-hash race to a tombstone. Two is already
/// generous: each retry needs a *further* concurrent removal to land
/// inside the same window.
const MAX_COMMIT_ATTEMPTS: u8 = 2;

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
        prepared: PreparedImport,
        budget: PersistBudget,
    ) -> Result<ImportOutcome, CoreError> {
        self.commit_import_hooked(prepared, budget, &|| {})
    }

    /// [`commit_import_budgeted`](Self::commit_import_budgeted) with a hook
    /// fired in the one window this stage cannot close from the inside: the
    /// instant before the writer lock is taken, with the staged files not
    /// yet placed. The production path passes a no-op; tests pass a closure
    /// that drives a concurrent removal into exactly that gap.
    pub(super) fn commit_import_hooked(
        &self,
        prepared: PreparedImport,
        budget: PersistBudget,
        before_lock: &dyn Fn(),
    ) -> Result<ImportOutcome, CoreError> {
        self.commit_import_attempt(prepared, budget, before_lock, 0)
    }

    /// One commit attempt. `attempt` bounds the one path that starts over:
    /// a commit that lost the content-hash race to a *tombstone* re-enters
    /// here as a restore (see the lost-race arm below). Each retry needs
    /// another concurrent removal to happen, so the bound is a guard rail,
    /// not a expected-to-be-hit limit.
    fn commit_import_attempt(
        &self,
        mut prepared: PreparedImport,
        mut budget: PersistBudget,
        before_lock: &dyn Fn(),
        attempt: u8,
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
        // A restore adopts a removed book's id, so every destination path
        // below is shared with whatever else holds that id; a fresh import
        // minted its own UUID and owns its paths alone. That difference
        // decides when the files are placed, and it is the only thing this
        // flag means.
        let restoring = restore.is_some();
        let final_path = self.data_dir.join(&prepared.rel_path);
        // Taken out so the cleanup closures can hold it while the insert
        // path moves the rest of `prepared` into the row.
        let staged_book = std::mem::take(&mut prepared.tmp_path);

        // `(relative path for the row, staged temp awaiting its rename)`.
        // The temp is `Some` only on a restore.
        let (cover_rel, staged_cover) = match &prepared.cover {
            Some(cover) => {
                let rel = format!("covers/{}.{}", prepared.id, cover.extension);
                let cover_path = self.data_dir.join(&rel);
                // On a restore that final path may already hold a live
                // row's only cover, so the bytes go to a name nothing else
                // can spell and move into place once the row is claimed.
                let (write_path, staged) = if restoring {
                    let tmp = self
                        .data_dir
                        .join(format!("covers/{}.tmp", uuid::Uuid::new_v4()));
                    (tmp.clone(), Some((tmp, cover_path)))
                } else {
                    (cover_path, None)
                };
                if let Err(e) = std::fs::write(&write_path, &cover.bytes) {
                    // A write that fails part-way still leaves a partial
                    // file, and on a fresh import `cover_rel` never binds,
                    // so `cleanup_files` below could not reach it: sweep it
                    // here or it lingers unreferenced until the next
                    // Library::open. On a restore this is our own temp.
                    let _ = std::fs::remove_file(&write_path);
                    let _ = std::fs::remove_file(&staged_book);
                    return Err(e.into());
                }
                (Some(rel), staged)
            }
            None => (None, None),
        };
        let cleanup_files = |include_book: bool| {
            if include_book {
                let _ = std::fs::remove_file(&final_path);
            }
            if let Some(rel) = &cover_rel {
                let _ = std::fs::remove_file(self.data_dir.join(rel));
            }
        };
        // A restore's own files, still under their staging names.
        let cleanup_staged = || {
            let _ = std::fs::remove_file(&staged_book);
            if let Some((tmp, _)) = &staged_cover {
                let _ = std::fs::remove_file(tmp);
            }
        };
        // Every failure path from here shares one rule: delete only what
        // this import owns. A fresh import owns what sits at its final
        // paths; a restore owns only its temps, because it places nothing
        // at the shared paths until it has claimed the row — and once it
        // has, whatever it placed belongs to a row it may have failed to
        // commit, which makes it unreferenced and the sweep's business,
        // not something to unlink out from under a concurrent winner.
        let cleanup = |include_book: bool| {
            if restoring {
                cleanup_staged();
            } else {
                cleanup_files(include_book);
            }
        };

        if !restoring {
            if let Err(e) = std::fs::rename(&staged_book, &final_path) {
                let _ = std::fs::remove_file(&staged_book);
                cleanup_files(false);
                return Err(e.into());
            }
            // `copy_and_hash` fsynced the bytes, but the rename that names
            // them only lives in the directory cache, so the
            // file-before-row ordering above is not durable until `books/`
            // is flushed. Doing it here — before the commit — is what keeps
            // a power loss from leaving a row whose book is gone; the
            // reverse (an unreferenced file) is swept at the next open. One
            // extra directory fsync per import is nothing beside the
            // whole-file copy, hash, and parse the import already paid, and
            // the book is the irreplaceable artifact. `covers/`
            // deliberately gets no such flush: a cover is derived data,
            // re-creatable from the book we just made durable.
            if let Err(e) = sync_dir(&self.data_dir.join("books")) {
                cleanup_files(true);
                return Err(e);
            }
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
                    if !restore::revive(
                        tx,
                        &publication,
                        &authors,
                        coordinates_restored,
                        prepared.edition_key.as_deref(),
                    )? {
                        return Ok(false);
                    }
                } else {
                    insert_publication(
                        tx,
                        &publication,
                        &authors,
                        &prepared.content_hash,
                        prepared.edition_key.as_deref(),
                    )?;
                }
                write_spine_rows(tx, budget, &prepared.spine, &publication.id)?;
                write_position_rows(tx, &position_rows, position_total, &publication.id)?;
                write_chapter_rows(tx, budget, &prepared.toc, &publication.id)?;
                Ok(true)
            };

        // The one window a concurrent `Library::remove` can occupy: the
        // files are staged and the lock is not yet held. Production passes
        // a no-op; a test drives the removal in here.
        before_lock();

        let claimed = {
            let mut conn = self.writer.lock().unwrap();
            // A fresh import's book and cover are already on disk, so every
            // early exit from here on must sweep them or they linger
            // unreferenced until the next Library::open.
            let tx = match conn.transaction() {
                Ok(tx) => tx,
                Err(e) => {
                    cleanup(true);
                    return Err(e.into());
                }
            };
            match insert(&tx, &mut budget) {
                Ok(true) => {
                    // The row is claimed and the writer lock still held, so
                    // a restore's files go to their shared final paths
                    // here: `Library::remove` claims the live row under
                    // this same lock before it unlinks, which puts it
                    // strictly before or strictly after this rename, never
                    // between it and the commit below.
                    if let Err(e) = place_restored_files(
                        restoring,
                        &staged_book,
                        &final_path,
                        &staged_cover,
                        &self.data_dir,
                    ) {
                        // Rolls the revive back; anything already renamed
                        // is left for the sweep (see `cleanup`).
                        drop(tx);
                        cleanup(true);
                        return Err(e);
                    }
                    if let Err(e) = tx.commit() {
                        cleanup(true);
                        return Err(e.into());
                    }
                    true
                }
                // A restore that found the tombstone already revived. It
                // placed nothing: the winner's files stay exactly as the
                // winner left them, and the temps go below.
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
                    cleanup(true);
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
            // between our dedupe check and this write. Read through the
            // write to learn what it committed, tombstones included — the
            // live-only lookup would report "no such hash" for content the
            // library demonstrably holds and surface a raw BLAKE3 digest to
            // the reader as `NotFound`.
            match self.match_by_hash(&prepared.content_hash)? {
                // The ordinary case: a live row holds the content.
                HashMatch::Live(existing) => {
                    cleanup(true);
                    Ok(ImportOutcome::Duplicate(*existing))
                }
                // The winner committed and the book was removed again (or
                // the winner was itself a restore that has since been
                // removed). These bytes still belong on that tombstone's
                // id — exactly what `prepare_staged` would have decided had
                // it looked one moment later — so this attempt starts over
                // as a restore rather than reporting a failure. The book
                // this attempt already has on disk is handed to the retry;
                // everything else it owns goes.
                HashMatch::Removed(tombstone) if attempt < MAX_COMMIT_ATTEMPTS => {
                    let source = if restoring {
                        if let Some((tmp, _)) = &staged_cover {
                            let _ = std::fs::remove_file(tmp);
                        }
                        staged_book.clone()
                    } else {
                        cleanup_files(false);
                        final_path.clone()
                    };
                    let retry = PreparedImport {
                        rel_path: format!("books/{}.epub", tombstone.id),
                        id: tombstone.id.clone(),
                        tmp_path: self
                            .data_dir
                            .join(format!("books/{}.tmp", uuid::Uuid::new_v4())),
                        content_hash: prepared.content_hash,
                        // Cloned, not moved: `publication` is this
                        // attempt's record and the closures above still
                        // borrow it. Four small strings on a path taken
                        // only when a removal lands inside the race window.
                        title: publication.title.clone(),
                        authors: publication.authors.clone(),
                        language: publication.language.clone(),
                        text_encoding: publication.text_encoding.clone(),
                        spine: prepared.spine,
                        toc: prepared.toc,
                        cover: prepared.cover,
                        edition_key: prepared.edition_key,
                        restore: Some(tombstone),
                    };
                    if let Err(e) = std::fs::rename(&source, &retry.tmp_path) {
                        let _ = std::fs::remove_file(&source);
                        return Err(e.into());
                    }
                    // A no-op hook: `before_lock` is this attempt's
                    // injected race, already fired and not to be re-run.
                    self.commit_import_attempt(retry, budget.restart(), &|| {}, attempt + 1)
                }
                // Out of retries, or the row vanished behind the core's
                // back (a hard DELETE no import path performs).
                _ => {
                    cleanup(true);
                    Err(CoreError::NotFound(prepared.content_hash))
                }
            }
        }
    }
}

/// Moves a restored book and cover from their staging names onto the
/// adopted publication's own paths, then flushes `books/`. A no-op for a
/// fresh import, which placed its files before the lock.
///
/// Called with the row already claimed and the writer lock held, so the
/// paths it writes are this import's to write. The `books/` flush is what
/// makes the rename durable ahead of the commit that follows, exactly as on
/// the fresh path; the cover, being derived data, needs none.
fn place_restored_files(
    restoring: bool,
    staged_book: &std::path::Path,
    final_path: &std::path::Path,
    staged_cover: &Option<(std::path::PathBuf, std::path::PathBuf)>,
    data_dir: &std::path::Path,
) -> Result<(), CoreError> {
    if !restoring {
        return Ok(());
    }
    std::fs::rename(staged_book, final_path)?;
    if let Some((tmp, dest)) = staged_cover {
        std::fs::rename(tmp, dest)?;
    }
    sync_dir(&data_dir.join("books"))
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
    edition_key: Option<&str>,
) -> Result<(), CoreError> {
    tx.execute(
        "INSERT INTO publications_all
            (id, title, authors, language, text_encoding, format, file_path,
             cover_path, content_hash, added_at, progression, reconciled_at,
             edition_key, title_key, edition_scanned_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
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
            edition_key,
            title_key(&publication.title),
            // Scanned by construction: the OPF this import just parsed IS
            // what the background backfill would re-open the file for, so
            // the pass skips the book. Stamped even when the key came out
            // `None` — a junk identifier is a finished scan, not a
            // pending one.
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
        "UPDATE publications_all SET position_count = ?1 WHERE id = ?2",
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
