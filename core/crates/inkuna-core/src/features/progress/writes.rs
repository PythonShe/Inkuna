//! The progress writes a reader session produces.

use inkuna_engine::Coordinate;

use super::positions::{position_for, position_ranges_on};
use crate::core::time::unix_now;
use crate::{CoreError, Library};

/// An end-of-book progression is not guaranteed to land on exactly 1.0,
/// so "finished" begins slightly early.
const FINISH_THRESHOLD: f64 = 0.995;

impl Library {
    /// One call per page turn: updates the publication's content
    /// coordinate, progression, and `last_opened_at`, plus — when this
    /// publication has an open session — the session's end-state and
    /// `updated_at` heartbeat, all in a single writer transaction. With
    /// no open session it updates the publication only; never an error.
    ///
    /// Shells may pass `position: None`: the core then derives the
    /// synthetic position from the coordinate against
    /// `resource_positions` (best-effort — a book with no rows leaves the
    /// session position untouched), so session stats keep flowing without
    /// a shell-side position model.
    ///
    /// Auto-finish is transition-triggered: `finished_at` is set only when
    /// this update crosses `FINISH_THRESHOLD` upward, so an explicit
    /// unfinish sticks while the reader stays at the end of the book.
    pub fn update_progress(
        &self,
        id: &str,
        coordinate: Coordinate,
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
             SET position_spine_idx = ?1, position_char_offset = ?2, progression = ?3,
                 last_opened_at = ?4, finished_at = ?5
             WHERE id = ?6",
            rusqlite::params![
                coordinate.spine_idx,
                coordinate.char_offset as i64,
                progression,
                now,
                finished_at,
                id
            ],
        )?;
        // Derive the synthetic position from the coordinate when the shell
        // did not pass one; best-effort — no position rows leaves it None.
        let position = match position {
            Some(position) => Some(position),
            None => position_for(&position_ranges_on(&tx, id)?, coordinate),
        };
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
