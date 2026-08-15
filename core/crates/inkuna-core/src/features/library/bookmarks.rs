//! Bookmarks: opaque Readium locators pinned to a point in a publication.

use super::model::Bookmark;
use super::Library;
use crate::core::time::unix_now;
use crate::CoreError;

impl Library {
    pub fn add_bookmark(
        &self,
        publication_id: &str,
        locator: &str,
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
            locator: locator.to_string(),
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
            "INSERT INTO bookmarks (id, publication_id, locator, progression, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                bookmark.id,
                bookmark.publication_id,
                bookmark.locator,
                bookmark.progression,
                bookmark.created_at,
            ],
        )?;
        Ok(bookmark)
    }

    /// Bookmarks for a publication, sorted by progression through the book.
    pub fn bookmarks(&self, publication_id: &str) -> Result<Vec<Bookmark>, CoreError> {
        self.readers.with(|conn| {
            let mut stmt = conn.prepare_cached(
                "SELECT id, publication_id, locator, progression, created_at
                 FROM bookmarks WHERE publication_id = ?1
                 ORDER BY progression, created_at, rowid",
            )?;
            let rows = stmt.query_map([publication_id], |row| {
                Ok(Bookmark {
                    id: row.get(0)?,
                    publication_id: row.get(1)?,
                    locator: row.get(2)?,
                    progression: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })?;
            rows.collect::<Result<_, _>>().map_err(Into::into)
        })
    }

    pub fn remove_bookmark(&self, bookmark_id: &str) -> Result<(), CoreError> {
        let conn = self.writer.lock().unwrap();
        let changed = conn.execute("DELETE FROM bookmarks WHERE id = ?1", [bookmark_id])?;
        if changed == 0 {
            return Err(CoreError::NotFound(bookmark_id.to_string()));
        }
        Ok(())
    }
}
