//! Reading and writing the single settings row.

use rusqlite::OptionalExtension;

use super::model::{Settings, DEFAULT_BRIGHTNESS, MAX_TEXT_SIZE_STEP};
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
                    "SELECT onboarded, reading_theme, text_size_step, brightness
                     FROM settings WHERE id = 1",
                    [],
                    |row| {
                        Ok(Settings {
                            onboarded: row.get(0)?,
                            reading_theme: row.get(1)?,
                            text_size_step: row.get(2)?,
                            brightness: row.get(3)?,
                        })
                    },
                )
                .optional()?;
            Ok(stored.unwrap_or_default())
        })
    }

    /// Whole-record write. The core clamps out-of-range values instead of
    /// erroring: `text_size_step` to 0..=4, `brightness` to 0.0..=1.0
    /// (non-finite falls back to the default).
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
        let conn = self.writer.lock().unwrap();
        conn.execute(
            "INSERT INTO settings (id, onboarded, reading_theme, text_size_step, brightness)
             VALUES (1, ?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET
                 onboarded      = excluded.onboarded,
                 reading_theme  = excluded.reading_theme,
                 text_size_step = excluded.text_size_step,
                 brightness     = excluded.brightness",
            rusqlite::params![
                settings.onboarded,
                settings.reading_theme,
                text_size_step,
                brightness,
            ],
        )?;
        Ok(())
    }
}
