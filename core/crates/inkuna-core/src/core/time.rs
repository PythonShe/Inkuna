//! Wall-clock helper: every timestamp the DB stores is unix seconds (`i64`).

pub(crate) fn unix_now() -> i64 {
    chrono::Utc::now().timestamp()
}
