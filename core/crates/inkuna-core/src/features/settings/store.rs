//! Reading and writing the single settings row.

use rusqlite::OptionalExtension;

use super::model::{
    Settings, DEFAULT_BRIGHTNESS, DEFAULT_LINE_SPACING, MAX_LETTER_SPACING, MAX_LINE_SPACING,
    MAX_READING_MARGINS, MAX_REMINDER_MINUTES, MAX_TEXT_SIZE_STEP, MAX_WORD_SPACING,
    MIN_LINE_SPACING, MIN_READING_MARGINS,
};
use crate::{CoreError, Library};

impl Library {
    /// Reads the whole settings record. The row is seeded by the schema
    /// migration, so it is always there in practice; if it ever is not,
    /// this returns the core-owned defaults rather than an error, because
    /// "one row with core-owned defaults" is the contract this module
    /// promises and a settings read has nothing better to fail with.
    pub fn settings(&self) -> Result<Settings, CoreError> {
        self.readers.with(|conn| {
            let stored = conn
                .query_row(
                    "SELECT onboarded, reading_theme, text_size_step, brightness,
                            evening_reminder, reminder_minutes, account_name, account_email,
                            reading_font, reading_bold, line_spacing, letter_spacing,
                            word_spacing, reading_margins
                     FROM settings WHERE id = 1",
                    [],
                    |row| {
                        Ok(Settings {
                            onboarded: row.get(0)?,
                            reading_theme: row.get(1)?,
                            text_size_step: row.get(2)?,
                            brightness: row.get(3)?,
                            evening_reminder: row.get(4)?,
                            reminder_minutes: row.get(5)?,
                            account_name: row.get(6)?,
                            account_email: row.get(7)?,
                            reading_font: row.get(8)?,
                            reading_bold: row.get(9)?,
                            line_spacing: row.get(10)?,
                            letter_spacing: row.get(11)?,
                            word_spacing: row.get(12)?,
                            reading_margins: row.get(13)?,
                        })
                    },
                )
                .optional()?;
            Ok(stored.unwrap_or_default())
        })
    }

    /// Whole-record write. The core clamps out-of-range values instead of
    /// erroring: `text_size_step` to 0..=4, `brightness` to 0.0..=1.0
    /// (non-finite falls back to the default), `reminder_minutes` to
    /// 0..=1439, `line_spacing` to 1.30..=2.10, `letter_spacing` to
    /// 0.0..=0.06, `word_spacing` to 0.0..=0.30 (non-finite spacings fall
    /// back to their defaults), and `reading_margins` to 16..=48.
    ///
    /// Written as an upsert so the write cannot silently do nothing: a
    /// plain `UPDATE ... WHERE id = 1` against a missing row returns
    /// `Ok(0)`, which would report success while dropping everything the
    /// caller asked to store. The `CHECK (id = 1)` in the schema keeps this
    /// a singleton either way.
    pub fn set_settings(&self, settings: &Settings) -> Result<(), CoreError> {
        let text_size_step = settings.text_size_step.min(MAX_TEXT_SIZE_STEP);
        let brightness = if settings.brightness.is_finite() {
            settings.brightness.clamp(0.0, 1.0)
        } else {
            DEFAULT_BRIGHTNESS
        };
        let reminder_minutes = settings.reminder_minutes.min(MAX_REMINDER_MINUTES);
        let line_spacing = if settings.line_spacing.is_finite() {
            settings
                .line_spacing
                .clamp(MIN_LINE_SPACING, MAX_LINE_SPACING)
        } else {
            DEFAULT_LINE_SPACING
        };
        let letter_spacing = if settings.letter_spacing.is_finite() {
            settings.letter_spacing.clamp(0.0, MAX_LETTER_SPACING)
        } else {
            0.0
        };
        let word_spacing = if settings.word_spacing.is_finite() {
            settings.word_spacing.clamp(0.0, MAX_WORD_SPACING)
        } else {
            0.0
        };
        let reading_margins = settings
            .reading_margins
            .clamp(MIN_READING_MARGINS, MAX_READING_MARGINS);
        let conn = self.writer.lock().unwrap();
        conn.execute(
            "INSERT INTO settings (id, onboarded, reading_theme, text_size_step, brightness,
                                   evening_reminder, reminder_minutes, account_name, account_email,
                                   reading_font, reading_bold, line_spacing, letter_spacing,
                                   word_spacing, reading_margins)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
             ON CONFLICT(id) DO UPDATE SET
                 onboarded        = excluded.onboarded,
                 reading_theme    = excluded.reading_theme,
                 text_size_step   = excluded.text_size_step,
                 brightness       = excluded.brightness,
                 evening_reminder = excluded.evening_reminder,
                 reminder_minutes = excluded.reminder_minutes,
                 account_name     = excluded.account_name,
                 account_email    = excluded.account_email,
                 reading_font     = excluded.reading_font,
                 reading_bold     = excluded.reading_bold,
                 line_spacing     = excluded.line_spacing,
                 letter_spacing   = excluded.letter_spacing,
                 word_spacing     = excluded.word_spacing,
                 reading_margins  = excluded.reading_margins",
            rusqlite::params![
                settings.onboarded,
                settings.reading_theme,
                text_size_step,
                brightness,
                settings.evening_reminder,
                reminder_minutes,
                settings.account_name,
                settings.account_email,
                settings.reading_font,
                settings.reading_bold,
                line_spacing,
                letter_spacing,
                word_spacing,
                reading_margins,
            ],
        )?;
        Ok(())
    }
}
