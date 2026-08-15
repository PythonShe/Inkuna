//! Reading progress: one write per page turn, Readium synthetic positions,
//! and finish semantics. `progression` is always the book-wide
//! `totalProgression` (0.0..=1.0), never per-resource; the locator blob is
//! opaque — stored and returned, never parsed.

use crate::library::unix_now;
use crate::{CoreError, Library};

/// Readium's end-of-book `totalProgression` is not guaranteed to land on
/// exactly 1.0, so "finished" begins slightly early.
const FINISH_THRESHOLD: f64 = 0.995;

impl Library {
    /// One call per page turn: updates the publication's locator,
    /// progression, and `last_opened_at`, plus — when this publication has
    /// an open session — the session's end-state and `updated_at`
    /// heartbeat, all in a single writer transaction. With no open session
    /// it updates the publication only; never an error.
    ///
    /// Auto-finish is transition-triggered: `finished_at` is set only when
    /// this update crosses `FINISH_THRESHOLD` upward, so an explicit
    /// unfinish sticks while the reader stays at the end of the book.
    pub fn update_progress(
        &self,
        id: &str,
        locator: &str,
        progression: f64,
        position: Option<u32>,
    ) -> Result<(), CoreError> {
        let mut conn = self.writer.lock().unwrap();
        let tx = conn.transaction()?;

        let previous: Option<(f64, Option<i64>)> = tx
            .query_row(
                "SELECT progression, finished_at FROM publications WHERE id = ?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        let Some((old_progression, finished_at)) = previous else {
            return Err(CoreError::NotFound(id.to_string()));
        };

        let progression = if progression.is_finite() {
            progression.clamp(0.0, 1.0)
        } else {
            old_progression
        };
        let now = unix_now();

        let finished_at = match finished_at {
            None if old_progression < FINISH_THRESHOLD && progression >= FINISH_THRESHOLD => {
                Some(now)
            }
            existing => existing,
        };

        tx.execute(
            "UPDATE publications
             SET locator = ?1, progression = ?2, last_opened_at = ?3, finished_at = ?4
             WHERE id = ?5",
            rusqlite::params![locator, progression, now, finished_at, id],
        )?;
        // Session heartbeat: the first position-bearing update of a session
        // also backfills its start_position, so pages-read deltas measure
        // the whole sitting.
        tx.execute(
            "UPDATE sessions
             SET end_progression = ?1,
                 start_position  = COALESCE(start_position, ?2),
                 end_position    = COALESCE(?2, end_position),
                 updated_at      = ?3
             WHERE publication_id = ?4 AND ended_at IS NULL",
            rusqlite::params![progression, position, now, id],
        )?;

        tx.commit()?;
        Ok(())
    }

    /// Records the navigator's synthetic position count, once known; from
    /// then on "page N of M" is real.
    pub fn report_position_count(&self, id: &str, count: u32) -> Result<(), CoreError> {
        let conn = self.writer.lock().unwrap();
        let changed = conn.execute(
            "UPDATE publications SET position_count = ?1 WHERE id = ?2",
            rusqlite::params![count, id],
        )?;
        if changed == 0 {
            return Err(CoreError::NotFound(id.to_string()));
        }
        Ok(())
    }

    /// Explicit finish/unfinish. Unfinishing sticks even at end-of-book
    /// because auto-finish requires an upward crossing of the threshold.
    pub fn set_finished(&self, id: &str, finished: bool) -> Result<(), CoreError> {
        let finished_at = finished.then(unix_now);
        let conn = self.writer.lock().unwrap();
        let changed = conn.execute(
            "UPDATE publications SET finished_at = ?1 WHERE id = ?2",
            rusqlite::params![finished_at, id],
        )?;
        if changed == 0 {
            return Err(CoreError::NotFound(id.to_string()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::library::tests::write_epub;
    use crate::{CoreError, ImportOutcome, Library};

    fn library_with_book() -> (tempfile::TempDir, Library, String) {
        let dir = tempfile::tempdir().unwrap();
        let epub = dir.path().join("book.epub");
        write_epub(&epub, "月光書房", "紫式部", "ja");
        let library = Library::open(dir.path().join("library")).unwrap();
        let id = match library.import(epub.to_str().unwrap()).unwrap() {
            ImportOutcome::Imported(p) => p.id,
            other => panic!("unexpected {other:?}"),
        };
        (dir, library, id)
    }

    #[test]
    fn update_progress_stores_locator_progression_and_positions() {
        let (_dir, library, id) = library_with_book();

        let locator = r#"{"href":"OEBPS/text/ch01.xhtml","locations":{"totalProgression":0.42,"position":12}}"#;
        library.update_progress(&id, locator, 0.42, Some(12)).unwrap();
        library.report_position_count(&id, 300).unwrap();

        let publication = library.publication(&id).unwrap();
        assert_eq!(publication.locator.as_deref(), Some(locator));
        assert_eq!(publication.progression, 0.42);
        assert_eq!(publication.position_count, Some(300));
        assert!(publication.last_opened_at.is_some());
        assert!(publication.finished_at.is_none());

        assert!(matches!(
            library.update_progress("missing", "{}", 0.5, None),
            Err(CoreError::NotFound(_))
        ));
    }

    #[test]
    fn auto_finish_fires_only_on_upward_crossing() {
        let (_dir, library, id) = library_with_book();

        library.update_progress(&id, "{}", 0.9, None).unwrap();
        assert!(library.publication(&id).unwrap().finished_at.is_none());

        // Crossing the threshold from below finishes.
        library.update_progress(&id, "{}", 0.997, None).unwrap();
        assert!(library.publication(&id).unwrap().finished_at.is_some());

        // Explicit unfinish sticks while staying at the end of the book.
        library.set_finished(&id, false).unwrap();
        library.update_progress(&id, "{}", 0.998, None).unwrap();
        library.update_progress(&id, "{}", 1.0, None).unwrap();
        assert!(library.publication(&id).unwrap().finished_at.is_none());

        // Leaving the end and re-reaching it crosses upward again.
        library.update_progress(&id, "{}", 0.5, None).unwrap();
        library.update_progress(&id, "{}", 1.0, None).unwrap();
        assert!(library.publication(&id).unwrap().finished_at.is_some());

        // Explicit finish/unfinish round-trips.
        library.set_finished(&id, false).unwrap();
        assert!(library.publication(&id).unwrap().finished_at.is_none());
        library.set_finished(&id, true).unwrap();
        assert!(library.publication(&id).unwrap().finished_at.is_some());
    }
}
