use chrono::{FixedOffset, TimeZone};

use crate::test_support::write_epub;
use crate::{ImportOutcome, Library};

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

#[test]
fn stats_fall_back_to_utc_on_out_of_range_offset() {
    let (_dir, library, id) = library_with_book();
    let now = ts(2026, 3, 3, 22, 0);
    insert_session(
        &library, &id,
        ts(2026, 3, 2, 10, 0), Some(ts(2026, 3, 2, 10, 30)), ts(2026, 3, 2, 10, 30),
        Some(10), Some(25),
    );

    // An offset whose seconds conversion overflows i32 must fall back to
    // UTC, not panic (debug) or wrap into a bogus offset (release).
    let overflowing = library.stats_overview_at(now, i32::MAX, true).unwrap();
    let utc = library.stats_overview_at(now, 0, true).unwrap();
    assert_eq!(overflowing, utc);
}
