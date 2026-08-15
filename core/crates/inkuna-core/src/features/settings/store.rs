//! Reading and writing the single settings row.

use super::model::{Settings, DEFAULT_BRIGHTNESS, MAX_TEXT_SIZE_STEP};
use crate::{CoreError, Library};

impl Library {
    pub fn settings(&self) -> Result<Settings, CoreError> {
        self.readers.with(|conn| {
            conn.query_row(
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
            .map_err(Into::into)
        })
    }

    /// Whole-record write. The core clamps out-of-range values instead of
    /// erroring: `text_size_step` to 0..=4, `brightness` to 0.0..=1.0
    /// (non-finite falls back to the default).
    pub fn set_settings(&self, settings: &Settings) -> Result<(), CoreError> {
        let text_size_step = settings.text_size_step.min(MAX_TEXT_SIZE_STEP);
        let brightness = if settings.brightness.is_finite() {
            settings.brightness.clamp(0.0, 1.0)
        } else {
            DEFAULT_BRIGHTNESS
        };
        let conn = self.writer.lock().unwrap();
        conn.execute(
            "UPDATE settings
             SET onboarded = ?1, reading_theme = ?2, text_size_step = ?3, brightness = ?4
             WHERE id = 1",
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
