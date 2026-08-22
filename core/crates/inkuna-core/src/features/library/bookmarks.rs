//! Bookmarks: content coordinates pinned to a point in a publication.

use inkuna_engine::Coordinate;

use super::model::Bookmark;
use super::Library;
use crate::core::time::unix_now;
use crate::CoreError;

impl Library {
    /// Pins `coordinate` to a publication. `progression` is the book-wide
    /// position the list sorts by: it is clamped to 0.0..=1.0, and a
    /// non-finite value becomes 0.0 rather than failing, because a bad
    /// number must not cost the reader the bookmark. The legacy `locator`
    /// column is NOT NULL, so new rows write it as `''`. Returns
    /// `NotFound` if no publication has that id.
    pub fn add_bookmark(
        &self,
        publication_id: &str,
        coordinate: Coordinate,
        progression: f64,
    ) -> Result<Bookmark, CoreError> {
        let progression = if progression.is_finite() {
            progression.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let bookmark = Bookmark {
            id: uuid::Uuid::new_v4().to_string(),
            publication_id: publication_id.to_string(),
            coordinate,
            progression,
            created_at: unix_now(),
        };
        let conn = self.writer.lock().unwrap();
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM publications WHERE id = ?1)",
            [publication_id],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(CoreError::NotFound(publication_id.to_string()));
        }
        conn.execute(
            "INSERT INTO bookmarks
                (id, publication_id, locator, position_spine_idx, position_char_offset,
                 progression, created_at)
             VALUES (?1, ?2, '', ?3, ?4, ?5, ?6)",
            rusqlite::params![
                bookmark.id,
                bookmark.publication_id,
                bookmark.coordinate.spine_idx,
                bookmark.coordinate.char_offset as i64,
                bookmark.progression,
                bookmark.created_at,
            ],
        )?;
        Ok(bookmark)
    }

    /// Bookmarks for a publication, sorted by progression through the
    /// book. A legacy row the V8 rebaseline has not converted yet reads
    /// as the book-start default `(0, 0)`.
    pub fn bookmarks(&self, publication_id: &str) -> Result<Vec<Bookmark>, CoreError> {
        self.readers.with(|conn| {
            let mut stmt = conn.prepare_cached(
                "SELECT id, publication_id, position_spine_idx, position_char_offset,
                        progression, created_at
                 FROM bookmarks WHERE publication_id = ?1
                 ORDER BY progression, created_at, rowid",
            )?;
            let rows = stmt.query_map([publication_id], |row| {
                let spine_idx: Option<u32> = row.get(2)?;
                let char_offset: Option<i64> = row.get(3)?;
                Ok(Bookmark {
                    id: row.get(0)?,
                    publication_id: row.get(1)?,
                    coordinate: Coordinate {
                        spine_idx: spine_idx.unwrap_or(0),
                        char_offset: char_offset.unwrap_or(0).max(0) as u64,
                    },
                    progression: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })?;
            rows.collect::<Result<_, _>>().map_err(Into::into)
        })
    }

    /// Deletes one bookmark by its own id. Deliberately not idempotent:
    /// removing a bookmark that is not there returns `NotFound`, so a
    /// shell swiping a stale row learns its list is out of date.
    pub fn remove_bookmark(&self, bookmark_id: &str) -> Result<(), CoreError> {
        let conn = self.writer.lock().unwrap();
        let changed = conn.execute("DELETE FROM bookmarks WHERE id = ?1", [bookmark_id])?;
        if changed == 0 {
            return Err(CoreError::NotFound(bookmark_id.to_string()));
        }
        Ok(())
    }
}
