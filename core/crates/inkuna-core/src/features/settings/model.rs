//! The settings record and the core-owned defaults behind it.

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
