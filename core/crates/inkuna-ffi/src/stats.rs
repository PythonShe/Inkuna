//! Reading sessions and the stats screen's numbers.

use crate::bookshelf::{blocking, Bookshelf};
use crate::error::InkunaError;

/// Numbers only — shells own formatting and localization.
#[derive(Debug, Clone, uniffi::Record)]
pub struct StatsOverview {
    pub pages_this_week: u32,
    pub minutes_this_month: u32,
    pub books_finished_this_year: u32,
    /// Day-of-month numbers of the current local month with ≥1 session.
    pub read_days: Vec<u8>,
}

impl From<inkuna_core::StatsOverview> for StatsOverview {
    fn from(s: inkuna_core::StatsOverview) -> Self {
        StatsOverview {
            pages_this_week: s.pages_this_week,
            minutes_this_month: s.minutes_this_month,
            books_finished_this_year: s.books_finished_this_year,
            read_days: s.read_days,
        }
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl Bookshelf {
    /// Starts a reading session at reader open; returns the session id.
    /// Also closes any crashed session for this publication retroactively.
    pub async fn session_start(&self, id: String) -> Result<String, InkunaError> {
        let library = self.0.clone();
        blocking(move || Ok(library.session_start(&id)?)).await
    }

    /// Ends a session at reader close / app background. Idempotent.
    pub async fn session_end(&self, session_id: String) -> Result<(), InkunaError> {
        let library = self.0.clone();
        blocking(move || Ok(library.session_end(&session_id)?)).await
    }

    /// Stats screen numbers in the caller's local time.
    ///
    /// `tz_offset_minutes` is **minutes** east of UTC, accepted range
    /// −1439..=1439 (e.g. 540 for JST, −480 for PST). An offset outside
    /// that range is not an error — the numbers are silently bucketed in
    /// UTC instead — so passing seconds by mistake yields plausible but
    /// wrong buckets rather than a failure.
    pub async fn stats_overview(
        &self,
        tz_offset_minutes: i32,
        week_starts_monday: bool,
    ) -> Result<StatsOverview, InkunaError> {
        let library = self.0.clone();
        blocking(move || {
            Ok(library
                .stats_overview(tz_offset_minutes, week_starts_monday)?
                .into())
        })
        .await
    }
}
