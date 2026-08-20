use super::model::{
    Settings, MAX_LETTER_SPACING, MAX_LINE_SPACING, MAX_READING_MARGINS, MAX_REMINDER_MINUTES,
    MAX_TEXT_SIZE_STEP,
};
use crate::Library;

#[test]
fn defaults_clamping_and_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let library = Library::open(dir.path().join("library")).unwrap();

    // Core-owned defaults on a fresh database.
    assert_eq!(library.settings().unwrap(), Settings::default());

    // Unknown theme ids are stored as-is (themes ship shell-first);
    // out-of-range values clamp on write.
    library
        .set_settings(&Settings {
            onboarded: true,
            reading_theme: "midnight-experiment".to_string(),
            text_size_step: 99,
            brightness: 7.5,
            evening_reminder: true,
            reminder_minutes: 40_000,
            account_name: "夜読み".to_string(),
            account_email: "reader@example.com".to_string(),
            // Unknown font ids are stored as-is (fonts ship shell-first);
            // spacings and margins clamp on write like the other numerics.
            reading_font: "brush-script-experiment".to_string(),
            reading_bold: true,
            line_spacing: 9.0,
            letter_spacing: 1.0,
            word_spacing: -2.0,
            reading_margins: 500,
        })
        .unwrap();
    let stored = library.settings().unwrap();
    assert!(stored.onboarded);
    assert_eq!(stored.reading_theme, "midnight-experiment");
    assert_eq!(stored.text_size_step, MAX_TEXT_SIZE_STEP);
    assert_eq!(stored.brightness, 1.0);
    assert!(stored.evening_reminder);
    assert_eq!(stored.reminder_minutes, MAX_REMINDER_MINUTES);
    assert_eq!(stored.account_name, "夜読み");
    assert_eq!(stored.account_email, "reader@example.com");
    assert_eq!(stored.reading_font, "brush-script-experiment");
    assert!(stored.reading_bold);
    assert_eq!(stored.line_spacing, MAX_LINE_SPACING);
    assert_eq!(stored.letter_spacing, MAX_LETTER_SPACING);
    assert_eq!(stored.word_spacing, 0.0);
    assert_eq!(stored.reading_margins, MAX_READING_MARGINS);

    // Settings survive reopen (single persistent row).
    drop(library);
    let library = Library::open(dir.path().join("library")).unwrap();
    assert!(library.settings().unwrap().onboarded);
}

/// The singleton row is seeded by the migration and nothing in the core
/// deletes it, but the contract is "core-owned defaults, read and written
/// whole" — so a missing row must read as the defaults and a write must
/// restore it, never report success while discarding the values.
#[test]
fn a_missing_settings_row_reads_as_defaults_and_is_restored_on_write() {
    let dir = tempfile::tempdir().unwrap();
    let library = Library::open(dir.path().join("library")).unwrap();

    library
        .writer
        .lock()
        .unwrap()
        .execute("DELETE FROM settings", [])
        .unwrap();

    assert_eq!(library.settings().unwrap(), Settings::default());

    library
        .set_settings(&Settings {
            onboarded: true,
            reading_theme: "sepia".to_string(),
            text_size_step: 3,
            brightness: 0.5,
            // Non-default values on every field, so a restore path that
            // silently dropped one could not still pass this test.
            evening_reminder: true,
            reminder_minutes: 6 * 60 + 30,
            account_name: "林深".to_string(),
            account_email: "shen@example.com".to_string(),
            reading_font: "noto-serif".to_string(),
            reading_bold: true,
            line_spacing: 1.4,
            letter_spacing: 0.02,
            word_spacing: 0.1,
            reading_margins: 40,
        })
        .unwrap();

    let stored = library.settings().unwrap();
    assert!(stored.onboarded);
    assert_eq!(stored.reading_theme, "sepia");
    assert_eq!(stored.text_size_step, 3);
    assert_eq!(stored.brightness, 0.5);
    assert!(stored.evening_reminder);
    assert_eq!(stored.reminder_minutes, 6 * 60 + 30);
    assert_eq!(stored.account_name, "林深");
    assert_eq!(stored.account_email, "shen@example.com");
    assert_eq!(stored.reading_font, "noto-serif");
    assert!(stored.reading_bold);
    assert_eq!(stored.line_spacing, 1.4);
    assert_eq!(stored.letter_spacing, 0.02);
    assert_eq!(stored.word_spacing, 0.1);
    assert_eq!(stored.reading_margins, 40);
}
