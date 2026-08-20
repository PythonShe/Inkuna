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
    /// Opaque reading-font identifier; shells own the font roster.
    pub reading_font: String,
    pub reading_bold: bool,
    /// Line-height multiplier (1.30..=2.10).
    pub line_spacing: f64,
    /// Extra letter spacing in em (0.0..=0.06).
    pub letter_spacing: f64,
    /// Extra word spacing in em (0.0..=0.30).
    pub word_spacing: f64,
    /// Horizontal page margins in CSS px (16..=48).
    pub reading_margins: u16,
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
            reading_font: s.reading_font,
            reading_bold: s.reading_bold,
            line_spacing: s.line_spacing,
            letter_spacing: s.letter_spacing,
            word_spacing: s.word_spacing,
            reading_margins: s.reading_margins,
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
            reading_font: s.reading_font,
            reading_bold: s.reading_bold,
            line_spacing: s.line_spacing,
            letter_spacing: s.letter_spacing,
            word_spacing: s.word_spacing,
            reading_margins: s.reading_margins,
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
