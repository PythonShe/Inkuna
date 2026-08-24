//! Read side: shelves, sort orders, single publications, the TOC, and
//! the spine.

use super::model::{map_publication, Chapter, Publication, Shelf, Sort, SpineEntry, PUB_COLUMNS};
use super::Library;
use crate::CoreError;

impl Library {
    /// One shelf's publications in `sort` order, filtered and ordered in
    /// SQL so a shell never sorts a library client-side. Ties break on
    /// `added_at` then rowid, making the order total and stable across
    /// calls; `Reading` means opened at least once and not finished, so a
    /// freshly imported book appears on `Unfinished` and `All` but not on
    /// `Reading`.
    pub fn list(&self, shelf: Shelf, sort: Sort) -> Result<Vec<Publication>, CoreError> {
        let filter = match shelf {
            Shelf::Reading => "WHERE last_opened_at IS NOT NULL AND finished_at IS NULL",
            Shelf::Unfinished => "WHERE finished_at IS NULL",
            Shelf::Finished => "WHERE finished_at IS NOT NULL",
            Shelf::All => "",
        };
        let order = match sort {
            Sort::RecentlyOpened => {
                "ORDER BY last_opened_at DESC NULLS LAST, added_at DESC, rowid DESC"
            }
            Sort::RecentlyAdded => "ORDER BY added_at DESC, rowid DESC",
        };
        self.readers.with(|conn| {
            let mut stmt = conn.prepare_cached(&format!(
                "SELECT {PUB_COLUMNS} FROM publications {filter} {order}"
            ))?;
            let rows = stmt.query_map([], map_publication)?;
            rows.collect::<Result<_, _>>().map_err(Into::into)
        })
    }

    /// One publication by id, including its current progress state.
    /// Returns `NotFound` when the row is gone (removed on another screen,
    /// or a stale id a shell held across a delete).
    pub fn publication(&self, id: &str) -> Result<Publication, CoreError> {
        self.readers.with(|conn| {
            let mut stmt = conn.prepare_cached(&format!(
                "SELECT {PUB_COLUMNS} FROM publications WHERE id = ?1"
            ))?;
            let mut rows = stmt.query_map([id], map_publication)?;
            match rows.next().transpose()? {
                Some(publication) => Ok(publication),
                None => Err(CoreError::NotFound(id.to_string())),
            }
        })
    }

    /// The flattened TOC in document order; empty for books without one
    /// (the text corpus is still complete — it keys off the spine).
    pub fn chapters(&self, id: &str) -> Result<Vec<Chapter>, CoreError> {
        let chapters: Vec<Chapter> = self.readers.with(|conn| {
            let mut stmt = conn.prepare_cached(
                "SELECT id, idx, title, href, depth FROM chapters
                 WHERE publication_id = ?1 ORDER BY idx",
            )?;
            let rows = stmt.query_map([id], |row| {
                Ok(Chapter {
                    id: row.get(0)?,
                    idx: row.get(1)?,
                    title: row.get(2)?,
                    href: row.get(3)?,
                    depth: row.get(4)?,
                })
            })?;
            rows.collect::<Result<_, _>>().map_err(Into::into)
        })?;
        if chapters.is_empty() {
            // Distinguish "no TOC" from "no such publication".
            self.publication(id)?;
        }
        Ok(chapters)
    }

    /// The publication's spine in reading order — the `spine_idx` →
    /// resource-href map a stored `Coordinate` needs when no reader
    /// session is open (naming the current chapter on a Home or Detail
    /// screen). Entry `n` always has `spine_idx == n`: the rows come back
    /// ordered and the import pipeline numbers them densely from 0, so a
    /// caller may index the `Vec` directly by a coordinate's `spine_idx`
    /// after a bounds check.
    ///
    /// Deliberately not folded into [`Chapter`]: the TOC-to-spine mapping
    /// is lossy in both directions — a chapter whose href matches no
    /// spine resource has no spine index at all, and one chapter may
    /// cover several spine items — so a `spine_idx` field on `Chapter`
    /// could not answer this honestly.
    ///
    /// Empty only for a publication with no spine rows; an unknown id
    /// throws `NotFound`.
    pub fn spine(&self, id: &str) -> Result<Vec<SpineEntry>, CoreError> {
        let spine: Vec<SpineEntry> = self.readers.with(|conn| {
            let mut stmt = conn.prepare_cached(
                "SELECT spine_idx, href FROM resources
                 WHERE publication_id = ?1 ORDER BY spine_idx",
            )?;
            let rows = stmt.query_map([id], |row| {
                Ok(SpineEntry {
                    spine_idx: row.get(0)?,
                    href: row.get(1)?,
                })
            })?;
            rows.collect::<Result<_, _>>().map_err(Into::into)
        })?;
        if spine.is_empty() {
            // Distinguish "no spine rows" from "no such publication".
            self.publication(id)?;
        }
        Ok(spine)
    }
}
