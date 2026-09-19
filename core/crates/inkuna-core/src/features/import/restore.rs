//! Restore: what happens when a staged file's content hash belongs to a
//! book that was removed rather than to no book at all.
//!
//! Removal tombstones the publication row instead of deleting it, so the
//! `sessions` and `bookmarks` rows already point at that id. A restore
//! therefore moves no data: it adopts the tombstone's id, imports the book
//! onto it in full, and clears `removed_at`. Everything preserved at
//! removal reattaches by construction.
//!
//! The one thing that cannot just be handed back unexamined is the
//! coordinates. `content_hash` identifies the *pre-conversion* source
//! bytes, so a re-imported MOBI/AZW3/TXT is converted and projected again
//! by whatever build is running now — and a build that improved chapter
//! detection, entity unescaping, or the aggregate text budget yields a
//! different character stream from the same file. The stored offsets would
//! then silently name different words. The corpus digest stamped at removal
//! is what catches that: it is compared against the corpus this import just
//! built, and the coordinates reattach only on an exact match.

use std::path::Path;

use crate::core::time::unix_now;
use crate::features::library::{corpus_digest, map_publication, title_key, Library, PUB_COLUMNS};
use crate::{CoreError, Publication};

/// A removed publication whose `content_hash` a staged import just matched.
/// Its id is what a restore adopts — that is the whole mechanism: the
/// `sessions` and `bookmarks` rows already point at this id, so writing the
/// book back onto it reattaches every one of them with no data movement.
pub(super) struct Tombstone {
    pub(super) id: String,
    /// Digest of the canonical corpus this row's frozen coordinates (its
    /// own and its bookmarks') index, taken at removal from the
    /// `resource_text` rows just before they were deleted. `None` on a row
    /// with no corpus to digest, or one tombstoned by a build older than
    /// the column — both are unknown provenance, which is exactly what
    /// this check exists to refuse.
    pub(super) corpus_digest: Option<String>,
}

/// What the library already knows about a staged file's content hash.
pub(super) enum HashMatch {
    /// Nothing holds this content.
    Fresh,
    /// A live book holds it — an ordinary duplicate.
    Live(Box<Publication>),
    /// A removed book held it, and its reading history is still there.
    Removed(Tombstone),
}

/// Whether the coordinates frozen on `tombstone` still address the same
/// characters, given the corpus this import just extracted.
///
/// `spine` is the fresh `(href, text)` corpus in spine order, so its
/// positions ARE the `spine_idx` values a coordinate carries. Anything
/// other than an exact digest match — a different stream, or a tombstone
/// that never recorded one — is a refusal, because the cost of being wrong
/// is a reader dropped into the wrong passage with no way to tell.
pub(super) fn coordinates_survive(
    tombstone: &Tombstone,
    spine: &[(String, Option<String>)],
) -> bool {
    let Some(stamped) = tombstone.corpus_digest.as_deref() else {
        return false;
    };
    let rebuilt = corpus_digest(
        spine
            .iter()
            .enumerate()
            .map(|(idx, (_, text))| (idx as u32, text.as_deref())),
    );
    stamped == rebuilt
}

/// Revives the tombstone in place, returning whether this import claimed
/// it. `false` means a concurrent import of the same content got there
/// first; the caller resolves that to `Duplicate`, exactly as the unique
/// index resolves the same race on a fresh insert.
///
/// The row's id, `content_hash`, `sessions`, `bookmarks`, and every
/// progress column (`progression`, the coordinate pair, `finished_at`,
/// `last_opened_at`) are deliberately left alone — that is the history
/// being handed back. `added_at` is refreshed because re-adding a book is
/// an addition: a book removed a year ago must not reappear buried at the
/// bottom of Recently Added, where the reader who just imported it would
/// read the import as having failed.
///
/// `reconciled_at` is stamped for the same reason a fresh import stamps
/// it: the corpus about to be written IS the canonical projection. The
/// exception is a book still holding an unconsumed legacy `locator` — a
/// pre-V8 book removed before the rebaseline ever reached it. That locator
/// is the only record of where its reader was, so the stamp is withheld
/// and the rebaseline converts it at the next open. Reads meanwhile see no
/// coordinate and fall back to progression, which is the fail-safe
/// direction.
///
/// "Unconsumed" covers the bookmarks as well as the publication row: the
/// schema permits either half to be legacy on its own (a pre-V8 book that
/// was bookmarked but never progress-written has a NULL publication
/// locator and nonempty bookmark ones), and the rebaseline is a whole-book
/// pass gated on this one stamp — stamping on the publication locator
/// alone would retire the pass before step 4 ever converted those
/// bookmarks, stranding them without coordinates for good.
/// `bookmarks.locator` is NOT NULL, so `<> ''` is its unconsumed test,
/// matching the rebaseline's own.
/// The edition keys are refreshed alongside `title`: they are derived from
/// the file that just arrived, and a tombstone's stored `title_key` was
/// derived from whichever title the *removed* copy carried. Leaving either
/// stale would desync it from the title beside it. `edition_scanned_at` is
/// stamped for the same reason a fresh import stamps it — this import
/// parsed the OPF, so the background backfill has nothing left to do here.
pub(super) fn revive(
    tx: &rusqlite::Transaction,
    publication: &Publication,
    authors: &str,
    coordinates_restored: bool,
    edition_key: Option<&str>,
) -> Result<bool, CoreError> {
    let claimed = tx.execute(
        "UPDATE publications_all
            SET title = ?1, authors = ?2, language = ?3, text_encoding = ?4,
                format = ?5, file_path = ?6, cover_path = ?7, added_at = ?8,
                removed_at = NULL, corpus_digest = NULL,
                edition_key = ?11, title_key = ?12, edition_scanned_at = ?9,
                reconciled_at = CASE
                    WHEN (locator IS NULL OR locator = '')
                     AND NOT EXISTS (
                         SELECT 1 FROM bookmarks b
                          WHERE b.publication_id = publications_all.id
                            AND b.locator <> ''
                     ) THEN ?9
                    ELSE NULL
                END
          WHERE id = ?10 AND removed_at IS NOT NULL",
        rusqlite::params![
            publication.title,
            authors,
            publication.language,
            publication.text_encoding,
            publication.format.as_str(),
            publication.file_path,
            publication.cover_path,
            publication.added_at,
            unix_now(),
            publication.id,
            edition_key,
            title_key(&publication.title),
        ],
    )?;
    if claimed == 0 {
        return Ok(false);
    }
    if !coordinates_restored {
        // The projection moved under the stored offsets, so they no longer
        // name the same characters. Drop them rather than reopen the book
        // at the wrong words; `progression` (and each bookmark's own)
        // stays, and every consumer already falls back to it when a
        // coordinate is absent.
        tx.execute(
            "UPDATE publications_all
                SET position_spine_idx = NULL, position_char_offset = NULL
              WHERE id = ?1",
            [&publication.id],
        )?;
        tx.execute(
            "UPDATE bookmarks
                SET position_spine_idx = NULL, position_char_offset = NULL
              WHERE publication_id = ?1",
            [&publication.id],
        )?;
    }
    // Derived rows are dropped at removal, so these are no-ops in the
    // ordinary case; they make the restore idempotent against a
    // half-finished earlier attempt.
    for table in ["resources", "chapters", "resource_positions"] {
        tx.execute(
            &format!("DELETE FROM {table} WHERE publication_id = ?1"),
            [&publication.id],
        )?;
    }
    Ok(true)
}

impl Library {
    /// Looks the content hash up in the library, sweeping the staged
    /// `.tmp` on a lookup error or on a *live* hit — the one case with
    /// nothing left to import. A tombstone hit keeps the staged file: it
    /// is about to be restored onto the removed book's id.
    pub(super) fn match_staged(
        &self,
        content_hash: &str,
        tmp_path: &Path,
    ) -> Result<HashMatch, CoreError> {
        let matched = self.match_by_hash(content_hash).inspect_err(|_| {
            let _ = std::fs::remove_file(tmp_path);
        })?;
        if matches!(matched, HashMatch::Live(_)) {
            let _ = std::fs::remove_file(tmp_path);
        }
        Ok(matched)
    }

    /// Everything the library knows about this content hash: nothing, a
    /// live duplicate, or a tombstone to restore onto. `content_hash` is
    /// uniquely indexed, so there is at most one row either way.
    pub(super) fn match_by_hash(&self, hash: &str) -> Result<HashMatch, CoreError> {
        self.readers.with(|conn| {
            let mut stmt = conn.prepare_cached(&format!(
                "SELECT {PUB_COLUMNS}, removed_at, corpus_digest
                 FROM publications_all WHERE content_hash = ?1"
            ))?;
            let mut rows = stmt.query_map([hash], |row| {
                // `PUB_COLUMNS` supplies indices 0..=14.
                let removed_at: Option<i64> = row.get(15)?;
                match removed_at {
                    None => Ok(HashMatch::Live(Box::new(map_publication(row)?))),
                    Some(_) => Ok(HashMatch::Removed(Tombstone {
                        id: row.get(0)?,
                        corpus_digest: row.get(16)?,
                    })),
                }
            })?;
            Ok(rows.next().transpose()?.unwrap_or(HashMatch::Fresh))
        })
    }
}
