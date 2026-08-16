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
}

impl From<inkuna_core::Settings> for Settings {
    fn from(s: inkuna_core::Settings) -> Self {
        Settings {
            onboarded: s.onboarded,
            reading_theme: s.reading_theme,
            text_size_step: s.text_size_step,
            brightness: s.brightness,
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
