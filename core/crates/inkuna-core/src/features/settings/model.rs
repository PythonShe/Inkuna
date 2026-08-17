//! The settings record and the core-owned defaults behind it.

/// Theme a fresh install reads in. An opaque identifier: the core stores
/// whatever string it is given, including ids no shell knows yet.
pub const DEFAULT_READING_THEME: &str = "paper";
/// Middle of the 0..=[`MAX_TEXT_SIZE_STEP`] scale — the size a reader who
/// never opens the type sheet gets.
pub const DEFAULT_TEXT_SIZE_STEP: u8 = 2;
/// Default reading brightness, in 0.0..=1.0.
pub const DEFAULT_BRIGHTNESS: f64 = 0.78;
/// Largest accepted `text_size_step`; `set_settings` clamps to it rather
/// than rejecting, so a shell that grows its scale first cannot fail a
/// write against an older core.
pub const MAX_TEXT_SIZE_STEP: u8 = 4;

/// The whole app-settings record: one core-owned row, read and written
/// whole. Values are clamped on write, never rejected, and unknown
/// `reading_theme` ids are stored as-is so themes can ship shell-first.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// Whether onboarding has been completed at least once.
    pub onboarded: bool,
    /// Opaque theme identifier; shells own the palettes.
    pub reading_theme: String,
    /// Type-size step in 0..=[`MAX_TEXT_SIZE_STEP`].
    pub text_size_step: u8,
    /// In-app reading brightness in 0.0..=1.0.
    pub brightness: f64,
    /// Whether the daily evening reading reminder is enabled. Scheduling
    /// is shell work; the core only remembers the choice.
    pub evening_reminder: bool,
    /// Display name of the purely local account; empty means "not set".
    pub account_name: String,
    /// Contact email of the purely local account; empty means "not set".
    /// Stored as-is — there is no server to validate against.
    pub account_email: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            onboarded: false,
            reading_theme: DEFAULT_READING_THEME.to_string(),
            text_size_step: DEFAULT_TEXT_SIZE_STEP,
            brightness: DEFAULT_BRIGHTNESS,
            evening_reminder: false,
            account_name: String::new(),
            account_email: String::new(),
        }
    }
}
