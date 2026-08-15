//! Session lifecycle: one row per sitting, opened at reader open and
//! closed at reader close — or retroactively, after a crash.

use crate::core::time::unix_now;
use crate::{CoreError, Library};

impl Library {
    /// Starts a reading session: retroactively closes any open session for
    /// this publication (`ended_at = updated_at` — the crash-recovery
    /// rule, applied lazily; this enforces the one-open-session
    /// invariant), snapshots current progression/position as start-state,
    /// and stamps `last_opened_at`. Returns the session id.
    pub fn session_start(&self, publication_id: &str) -> Result<String, CoreError> {
        let mut conn = self.writer.lock().unwrap();
        let tx = conn.transaction()?;

        let progression: f64 = tx
            .query_row(
                "SELECT progression FROM publications WHERE id = ?1",
                [publication_id],
                |row| row.get(0),
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    CoreError::NotFound(publication_id.to_string())
                }
                other => other.into(),
            })?;

        tx.execute(
            "UPDATE sessions SET ended_at = updated_at
             WHERE publication_id = ?1 AND ended_at IS NULL",
            [publication_id],
        )?;

        // The current position is whatever the last session's heartbeats
        // recorded; positions are layout-independent so it never goes
        // stale. NULL until the navigator first reports one.
        let start_position: Option<i64> = tx
            .query_row(
                "SELECT end_position FROM sessions WHERE publication_id = ?1
                 ORDER BY started_at DESC, rowid DESC LIMIT 1",
                [publication_id],
                |row| row.get(0),
            )
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;

        let session_id = uuid::Uuid::new_v4().to_string();
        let now = unix_now();
        tx.execute(
            "INSERT INTO sessions
                (id, publication_id, started_at, ended_at, updated_at,
                 start_progression, end_progression, start_position, end_position)
             VALUES (?1, ?2, ?3, NULL, ?3, ?4, ?4, ?5, ?5)",
            rusqlite::params![session_id, publication_id, now, progression, start_position],
        )?;
        tx.execute(
            "UPDATE publications SET last_opened_at = ?1 WHERE id = ?2",
            rusqlite::params![now, publication_id],
        )?;

        tx.commit()?;
        Ok(session_id)
    }

    /// Ends a session at now (idle time within a session counts as reading
    /// time; shells end sessions on background). Idempotent: a session
    /// already closed — for example retroactively by a later
    /// `session_start` — is left untouched.
    pub fn session_end(&self, session_id: &str) -> Result<(), CoreError> {
        let now = unix_now();
        let conn = self.writer.lock().unwrap();
        let changed = conn.execute(
            "UPDATE sessions SET ended_at = ?1, updated_at = ?1
             WHERE id = ?2 AND ended_at IS NULL",
            rusqlite::params![now, session_id],
        )?;
        if changed == 0 {
            let exists: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM sessions WHERE id = ?1)",
                [session_id],
                |row| row.get(0),
            )?;
            if !exists {
                return Err(CoreError::NotFound(session_id.to_string()));
            }
        }
        Ok(())
    }
}
