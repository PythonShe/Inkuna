//! Reading sessions and the stats screen's numbers.

use crate::bookshelf::blocking;
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

/// First day of the caller's week — all seven, because
/// `Calendar.firstWeekday` / `WeekFields.firstDayOfWeek` span all seven
/// (ar-EG weeks start on Saturday).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Weekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

impl From<Weekday> for inkuna_core::Weekday {
    fn from(day: Weekday) -> Self {
        match day {
            Weekday::Monday => inkuna_core::Weekday::Mon,
            Weekday::Tuesday => inkuna_core::Weekday::Tue,
            Weekday::Wednesday => inkuna_core::Weekday::Wed,
            Weekday::Thursday => inkuna_core::Weekday::Thu,
            Weekday::Friday => inkuna_core::Weekday::Fri,
            Weekday::Saturday => inkuna_core::Weekday::Sat,
            Weekday::Sunday => inkuna_core::Weekday::Sun,
        }
    }
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

/// The stats facade: reading sessions and the stats overview.
/// Constructed once by [`Bookshelf::open`], handed out by
/// `Bookshelf::stats()` as a cheap `Arc` clone.
#[derive(uniffi::Object)]
pub struct ShelfStats(pub(crate) std::sync::Arc<inkuna_core::Library>);

#[uniffi::export(async_runtime = "tokio")]
impl ShelfStats {
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

    /// Stats screen numbers in the caller's local calendar.
    ///
    /// `timezone` is an IANA zone id (`Asia/Tokyo`) so day, week, month,
    /// and year boundaries stay honest across DST transitions — a fixed
    /// offset cannot do that. An unknown id is not an error: the numbers
    /// are silently bucketed in UTC instead. `week_start` is the
    /// locale's first day of the week.
    pub async fn stats_overview(
        &self,
        timezone: String,
        week_start: Weekday,
    ) -> Result<StatsOverview, InkunaError> {
        let library = self.0.clone();
        blocking(move || Ok(library.stats_overview(&timezone, week_start.into())?.into())).await
    }
}
