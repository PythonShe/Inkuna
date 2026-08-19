//! App settings, read and written as one record.

use crate::bookshelf::{blocking, Bookshelf};
use crate::error::InkunaError;

#[derive(Debug, Clone, uniffi::Record)]
pub struct Settings {
    pub onboarded: bool,
    /// Opaque theme identifier; shells own the palettes.
    pub reading_theme: String,
    pub text_size_step: u8,
    pub brightness: f64,
    /// Daily evening reading reminder; scheduling is shell work.
    pub evening_reminder: bool,
    /// When the reminder fires, in minutes after local midnight (0..=1439).
    pub reminder_minutes: u16,
    /// Purely local account profile; empty strings mean "not set".
    pub account_name: String,
    pub account_email: String,
}

impl From<inkuna_core::Settings> for Settings {
    fn from(s: inkuna_core::Settings) -> Self {
        Settings {
            onboarded: s.onboarded,
            reading_theme: s.reading_theme,
            text_size_step: s.text_size_step,
            brightness: s.brightness,
            evening_reminder: s.evening_reminder,
            reminder_minutes: s.reminder_minutes,
            account_name: s.account_name,
            account_email: s.account_email,
        }
    }
}

impl From<Settings> for inkuna_core::Settings {
    fn from(s: Settings) -> Self {
        inkuna_core::Settings {
            onboarded: s.onboarded,
            reading_theme: s.reading_theme,
            text_size_step: s.text_size_step,
            brightness: s.brightness,
            evening_reminder: s.evening_reminder,
            reminder_minutes: s.reminder_minutes,
            account_name: s.account_name,
            account_email: s.account_email,
        }
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl Bookshelf {
    pub async fn settings(&self) -> Result<Settings, InkunaError> {
        let library = self.0.clone();
        blocking(move || Ok(library.settings()?.into())).await
    }

    /// Whole-record write; the core clamps out-of-range values.
    pub async fn set_settings(&self, settings: Settings) -> Result<(), InkunaError> {
        let library = self.0.clone();
        blocking(move || Ok(library.set_settings(&settings.into())?)).await
    }
}
