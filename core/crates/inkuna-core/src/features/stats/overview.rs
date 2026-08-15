//! The Stats screen's numbers, bucketed into the caller's local calendar.

use chrono::{Datelike, Duration, FixedOffset, Offset, Utc};

use crate::core::time::unix_now;
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
    /// The Stats screen's numbers, computed in the caller's local time
    /// (`tz_offset_minutes` east of UTC, accepted range −1439..=1439). A
    /// session belongs wholly to the local day/week/month of its
    /// `started_at` — one sitting, one bucket. An offset outside the
    /// accepted range is not an error: the numbers are bucketed in UTC.
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
        // `tz_offset_minutes` is minutes east of UTC and must land within
        // ±1439; anything else silently buckets in UTC rather than
        // failing an otherwise valid stats query. The UTC fallback is
        // infallible, so there is no error path here.
        let tz = tz_offset_minutes
            .checked_mul(60)
            .and_then(FixedOffset::east_opt)
            .unwrap_or_else(|| Utc.fix());
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
