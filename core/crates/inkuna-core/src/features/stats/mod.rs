//! Reading sessions and the stats overview. A session is one sitting:
//! `session_start` at reader open, heartbeats via `update_progress`,
//! `session_end` at close/background. Every number on the Stats screen
//! comes from here — numbers only, never formatted strings; shells own
//! formatting and localization.

mod overview;
mod sessions;

#[cfg(test)]
mod tests;

pub use overview::StatsOverview;
