//! Reading sessions and the stats overview. A session is one sitting:
//! `session_start` at reader open, heartbeats via `update_progress`,
//! `session_end` at close/background. Every number on the Stats screen
//! comes from here — numbers only, never formatted strings; shells own
//! formatting and localization.

use chrono::{Datelike, Duration, FixedOffset};

use crate::library::unix_now;
use crate::{CoreError, Library};

#[derive(Debug, Clone, PartialEq)]
pub struct StatsOverview {
    /// Σ over this week's sessions of max(0, end_position −
    /// start_position); sessions with NULL positions contribute 0 pages.
    pub pages_this_week: u32,
    pub minutes_this_month: u32,
    pub books_finished_this_year: u32,
    /// Day-of-month numbers (current local month) having at least one
    /// session.
    pub read_days: Vec<u8>,
}

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

    /// The Stats screen's numbers, computed in the caller's local time
    /// (`tz_offset_minutes` east of UTC). A session belongs wholly to the
    /// local day/week/month of its `started_at` — one sitting, one bucket.
    pub fn stats_overview(
        &self,
        tz_offset_minutes: i32,
        week_starts_monday: bool,
    ) -> Result<StatsOverview, CoreError> {
        self.stats_overview_at(unix_now(), tz_offset_minutes, week_starts_monday)
    }

    pub(crate) fn stats_overview_at(
        &self,
        now: i64,
        tz_offset_minutes: i32,
        week_starts_monday: bool,
    ) -> Result<StatsOverview, CoreError> {
        // An out-of-range offset (the API bound is ±24h) falls back to UTC.
        let tz = FixedOffset::east_opt(tz_offset_minutes * 60)
            .or_else(|| FixedOffset::east_opt(0))
            .ok_or_else(|| CoreError::NotFound("invalid utc offset".into()))?;
        let offset_seconds = i64::from(tz.local_minus_utc());
        // Local midnight of a NaiveDate, as a unix timestamp.
        let local_midnight_ts =
            |date: chrono::NaiveDate| date.and_time(chrono::NaiveTime::MIN).and_utc().timestamp() - offset_seconds;

        let today = match chrono::DateTime::from_timestamp(now, 0) {
            Some(dt) => dt.with_timezone(&tz).date_naive(),
            None => return Err(CoreError::NotFound("timestamp out of range".into())),
        };
        let days_into_week = if week_starts_monday {
            today.weekday().num_days_from_monday()
        } else {
            today.weekday().num_days_from_sunday()
        };
        let week_start = local_midnight_ts(today - Duration::days(i64::from(days_into_week)));
        let month_first = today.with_day(1).unwrap_or(today);
        let month_start = local_midnight_ts(month_first);
        let year_first = month_first.with_month(1).unwrap_or(month_first);
        let year_start = local_midnight_ts(year_first);

        struct SessionRow {
            started_at: i64,
            ended_at: Option<i64>,
            updated_at: i64,
            start_position: Option<i64>,
            end_position: Option<i64>,
        }

        let horizon = week_start.min(month_start);
        let (sessions, finished): (Vec<SessionRow>, u32) = self.readers.with(|conn| {
            let mut stmt = conn.prepare_cached(
                "SELECT started_at, ended_at, updated_at, start_position, end_position
                 FROM sessions WHERE started_at >= ?1",
            )?;
            let sessions = stmt
                .query_map([horizon], |row| {
                    Ok(SessionRow {
                        started_at: row.get(0)?,
                        ended_at: row.get(1)?,
                        updated_at: row.get(2)?,
                        start_position: row.get(3)?,
                        end_position: row.get(4)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            let finished: u32 = conn.query_row(
                "SELECT COUNT(*) FROM publications WHERE finished_at >= ?1",
                [year_start],
                |row| row.get(0),
            )?;
            Ok((sessions, finished))
        })?;

        let mut pages: u64 = 0;
        let mut seconds: u64 = 0;
        let mut days = std::collections::BTreeSet::new();
        for session in &sessions {
            if session.started_at >= week_start {
                if let (Some(start), Some(end)) = (session.start_position, session.end_position) {
                    // Paging backwards clamps to 0, never negative.
                    pages += (end - start).max(0) as u64;
                }
            }
            if session.started_at >= month_start {
                // Effective end covers in-flight sessions (viewed
                // mid-reading) and crashed ones not yet swept.
                let effective_end = session.ended_at.unwrap_or(session.updated_at);
                seconds += (effective_end - session.started_at).max(0) as u64;
                if let Some(dt) = chrono::DateTime::from_timestamp(session.started_at, 0) {
                    days.insert(dt.with_timezone(&tz).day() as u8);
                }
            }
        }

        Ok(StatsOverview {
            pages_this_week: pages.min(u32::MAX as u64) as u32,
            minutes_this_month: (seconds / 60).min(u32::MAX as u64) as u32,
            books_finished_this_year: finished,
            read_days: days.into_iter().collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::tests::write_epub;
    use crate::ImportOutcome;
    use chrono::TimeZone;

    const TOKYO_MINUTES: i32 = 9 * 60;

    fn ts(y: i32, m: u32, d: u32, h: u32, min: u32) -> i64 {
        FixedOffset::east_opt(TOKYO_MINUTES * 60)
            .unwrap()
            .with_ymd_and_hms(y, m, d, h, min, 0)
            .unwrap()
            .timestamp()
    }

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

    #[allow(clippy::too_many_arguments)]
    fn insert_session(
        library: &Library,
        publication_id: &str,
        started_at: i64,
        ended_at: Option<i64>,
        updated_at: i64,
        start_position: Option<i64>,
        end_position: Option<i64>,
    ) {
        let conn = library.writer.lock().unwrap();
        conn.execute(
            "INSERT INTO sessions
                (id, publication_id, started_at, ended_at, updated_at,
                 start_progression, end_progression, start_position, end_position)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, 0, ?6, ?7)",
            rusqlite::params![
                uuid::Uuid::new_v4().to_string(),
                publication_id,
                started_at,
                ended_at,
                updated_at,
                start_position,
                end_position,
            ],
        )
        .unwrap();
    }

    #[test]
    fn session_lifecycle_snapshots_and_crash_recovery() {
        let (_dir, library, id) = library_with_book();

        let first = library.session_start(&id).unwrap();
        library.update_progress(&id, "{}", 0.1, Some(12)).unwrap();

        // A second start (say, after a crash — session_end never came)
        // retroactively closes the first at its updated_at heartbeat and
        // snapshots the current position as the new start-state.
        let second = library.session_start(&id).unwrap();
        let (first_ended, first_updated): (Option<i64>, i64) = library
            .readers
            .with(|conn| {
                conn.query_row(
                    "SELECT ended_at, updated_at FROM sessions WHERE id = ?1",
                    [&first],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .map_err(Into::into)
            })
            .unwrap();
        assert_eq!(first_ended, Some(first_updated));

        let (second_start_pos, second_open): (Option<i64>, bool) = library
            .readers
            .with(|conn| {
                conn.query_row(
                    "SELECT start_position, ended_at IS NULL FROM sessions WHERE id = ?1",
                    [&second],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .map_err(Into::into)
            })
            .unwrap();
        assert_eq!(second_start_pos, Some(12));
        assert!(second_open);

        // last_opened_at stamped by session_start.
        assert!(library.publication(&id).unwrap().last_opened_at.is_some());

        // Ending is idempotent; ending an unknown session is NotFound.
        library.session_end(&second).unwrap();
        library.session_end(&second).unwrap();
        library.session_end(&first).unwrap();
        assert!(matches!(
            library.session_end("missing"),
            Err(crate::CoreError::NotFound(_))
        ));
    }

    #[test]
    fn stats_attribute_sessions_to_local_buckets() {
        let (_dir, library, id) = library_with_book();
        // "Now": Tuesday 2026-03-03 22:00 Tokyo. Monday week starts Mar 2;
        // Sunday week starts Mar 1; the month starts Mar 1.
        let now = ts(2026, 3, 3, 22, 0);

        // One sitting spanning local midnight Feb 28 → Mar 1: attributed
        // wholly to February — no minutes, no dots this month.
        insert_session(
            &library, &id,
            ts(2026, 2, 28, 23, 30), Some(ts(2026, 3, 1, 0, 30)), ts(2026, 3, 1, 0, 30),
            Some(1), Some(9),
        );
        // Sunday Mar 1, 20 minutes, 5 pages: this month, but before the
        // Monday-started week.
        insert_session(
            &library, &id,
            ts(2026, 3, 1, 8, 0), Some(ts(2026, 3, 1, 8, 20)), ts(2026, 3, 1, 8, 20),
            Some(100), Some(105),
        );
        // Monday Mar 2, 30 minutes, 15 pages: in week and month.
        insert_session(
            &library, &id,
            ts(2026, 3, 2, 10, 0), Some(ts(2026, 3, 2, 10, 30)), ts(2026, 3, 2, 10, 30),
            Some(10), Some(25),
        );
        // Tuesday Mar 3, 10 minutes, pages backwards: clamps to 0 pages.
        insert_session(
            &library, &id,
            ts(2026, 3, 3, 9, 0), Some(ts(2026, 3, 3, 9, 10)), ts(2026, 3, 3, 9, 10),
            Some(50), Some(40),
        );
        // In-flight tonight (no ended_at), NULL positions: counted via the
        // effective-end rule for minutes and dots, 0 pages.
        insert_session(
            &library, &id,
            now - 1200, None, now - 600,
            None, None,
        );

        library.set_finished(&id, true).unwrap();

        let monday = library
            .stats_overview_at(now, TOKYO_MINUTES, true)
            .unwrap();
        assert_eq!(monday.pages_this_week, 15);
        assert_eq!(monday.minutes_this_month, 20 + 30 + 10 + 10);
        assert_eq!(monday.books_finished_this_year, 1);
        assert_eq!(monday.read_days, vec![1, 2, 3]);

        // With a Sunday week start, Mar 1's 5 pages join the week.
        let sunday = library
            .stats_overview_at(now, TOKYO_MINUTES, false)
            .unwrap();
        assert_eq!(sunday.pages_this_week, 20);
    }
}
