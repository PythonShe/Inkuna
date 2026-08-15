//! Read side: shelves, sort orders, single publications, and the TOC.

use super::model::{map_publication, Chapter, Publication, Shelf, Sort, PUB_COLUMNS};
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
}
