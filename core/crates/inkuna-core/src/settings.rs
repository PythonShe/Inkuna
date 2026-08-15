//! App settings: a single core-owned row with core-owned defaults, read
//! and written whole. `reading_theme` is an opaque identifier — shells own
//! the palettes, and unknown ids are stored as-is so themes can ship
//! shell-first. New fields arrive by migration.

use crate::{CoreError, Library};

pub const DEFAULT_READING_THEME: &str = "paper";
pub const DEFAULT_TEXT_SIZE_STEP: u8 = 2;
pub const DEFAULT_BRIGHTNESS: f64 = 0.78;
pub const MAX_TEXT_SIZE_STEP: u8 = 4;

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub onboarded: bool,
    pub reading_theme: String,
    pub text_size_step: u8,
    pub brightness: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            onboarded: false,
            reading_theme: DEFAULT_READING_THEME.to_string(),
            text_size_step: DEFAULT_TEXT_SIZE_STEP,
            brightness: DEFAULT_BRIGHTNESS,
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

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
            })
            .unwrap();
        let stored = library.settings().unwrap();
        assert!(stored.onboarded);
        assert_eq!(stored.reading_theme, "midnight-experiment");
        assert_eq!(stored.text_size_step, MAX_TEXT_SIZE_STEP);
        assert_eq!(stored.brightness, 1.0);

        // Settings survive reopen (single persistent row).
        drop(library);
        let library = Library::open(dir.path().join("library")).unwrap();
        assert!(library.settings().unwrap().onboarded);
    }
}
