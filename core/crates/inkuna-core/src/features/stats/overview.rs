//! The Stats screen's numbers, bucketed into the caller's local calendar.

use chrono::offset::LocalResult;
use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveTime, Utc, Weekday};
use chrono_tz::Tz;

use crate::{CoreError, Library};

/// The Stats screen's numbers for one caller-local calendar. Numbers only
/// — shells own formatting and localization — and every bucket is derived
/// from the sessions table at call time, nothing is precomputed.
#[derive(Debug, Clone, PartialEq)]
pub struct StatsOverview {
    /// Σ over this week's sessions of max(0, end_position −
    /// start_position); sessions with NULL positions contribute 0 pages.
    pub pages_this_week: u32,
    /// Σ over this month's sessions of their duration, where an unclosed
    /// session ends at its last heartbeat (`updated_at`), so an in-flight
    /// or crashed sitting still counts.
    pub minutes_this_month: u32,
    /// Publications whose `finished_at` falls in the current local year.
    pub books_finished_this_year: u32,
    /// Day-of-month numbers (current local month) having at least one
    /// session.
    pub read_days: Vec<u8>,
}

impl Library {
    /// The Stats screen's numbers, computed in the caller's local
    /// calendar. `timezone` is an IANA zone id (`Asia/Tokyo`); bucketing
    /// with the zone rather than a fixed offset is what keeps week,
    /// month, and year boundaries honest across DST transitions — a
    /// year-start is a whole DST cycle away from today's offset. A
    /// session belongs wholly to the local day/week/month of its
    /// `started_at` — one sitting, one bucket. `week_start` is the
    /// locale's first day of week, any of the seven (ar-EG starts on
    /// Saturday). An unknown zone id is not an error: the numbers are
    /// bucketed in UTC.
    pub fn stats_overview(
        &self,
        timezone: &str,
        week_start: Weekday,
    ) -> Result<StatsOverview, CoreError> {
        self.stats_overview_at(Utc::now(), timezone, week_start)
    }

    /// [`stats_overview`](Self::stats_overview) against an explicit clock
    /// reading. `now` is a `DateTime`, not the crate's usual unix-seconds
    /// `i64`, because it is a clock reading rather than a stored value:
    /// carrying it already-converted is what keeps the bucketing total —
    /// an `i64` would have to be converted here, and every out-of-range
    /// `i64` would need an error the public entry point can never produce.
    pub(crate) fn stats_overview_at(
        &self,
        now: DateTime<Utc>,
        timezone: &str,
        week_start: Weekday,
    ) -> Result<StatsOverview, CoreError> {
        // An unknown zone id silently buckets in UTC rather than failing
        // an otherwise valid stats query; the fallback is infallible, so
        // there is no error path here.
        let tz: Tz = timezone.parse().unwrap_or(Tz::UTC);

        let today = now.with_timezone(&tz).date_naive();
        let days_into_week =
            (today.weekday().num_days_from_monday() + 7 - week_start.num_days_from_monday()) % 7;
        let week_start_ts = local_midnight(&tz, today - Duration::days(i64::from(days_into_week)));
        let month_first = today.with_day(1).unwrap_or(today);
        let month_start = local_midnight(&tz, month_first);
        let year_first = month_first.with_month(1).unwrap_or(month_first);
        let year_start = local_midnight(&tz, year_first);

        struct SessionRow {
            started_at: i64,
            ended_at: Option<i64>,
            updated_at: i64,
            start_position: Option<i64>,
            end_position: Option<i64>,
        }

        let horizon = week_start_ts.min(month_start);
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
            if session.started_at >= week_start_ts {
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

/// Local midnight of `date` in `tz`, as a unix timestamp. At a DST
/// transition an ambiguous midnight takes the earlier reading, and a
/// skipped midnight rolls forward to the first instant that exists —
/// scanning hour by hour, because the gap is not always one hour: a
/// handful of zones spring forward at 00:00, and Pacific/Apia skipped
/// December 30, 2011 *entirely* when Samoa crossed the date line, so the
/// first valid instant can be a whole day away.
fn local_midnight(tz: &Tz, date: NaiveDate) -> i64 {
    let midnight = date.and_time(NaiveTime::MIN);
    for hour in 0..=48 {
        match (midnight + Duration::hours(hour)).and_local_timezone(*tz) {
            LocalResult::Single(dt) | LocalResult::Ambiguous(dt, _) => return dt.timestamp(),
            LocalResult::None => {}
        }
    }
    // No tz database gap exceeds 48 hours; unreachable in practice.
    midnight.and_utc().timestamp()
}
