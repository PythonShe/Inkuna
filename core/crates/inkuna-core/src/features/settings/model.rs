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
/// Default reminder time, in minutes after midnight: 21:00, the fixed
/// "evening" the shells shipped with before the hour became configurable.
pub const DEFAULT_REMINDER_MINUTES: u16 = 21 * 60;
/// Largest accepted `reminder_minutes` (23:59); `set_settings` clamps to
/// it rather than rejecting.
pub const MAX_REMINDER_MINUTES: u16 = 23 * 60 + 59;
/// Reading font a fresh install gets: the publication's own faces. An
/// opaque identifier like `reading_theme` — the core stores whatever id it
/// is given so fonts can ship shell-first.
pub const DEFAULT_READING_FONT: &str = "publisher";
/// Default reading line-height multiplier (unitless).
pub const DEFAULT_LINE_SPACING: f64 = 1.65;
/// `line_spacing` bounds; `set_settings` clamps rather than rejects.
pub const MIN_LINE_SPACING: f64 = 1.30;
pub const MAX_LINE_SPACING: f64 = 2.10;
/// Largest accepted `letter_spacing`, in em.
pub const MAX_LETTER_SPACING: f64 = 0.06;
/// Largest accepted `word_spacing`, in em.
pub const MAX_WORD_SPACING: f64 = 0.30;
/// Default horizontal reading margins, in CSS px inside the rendering
/// web view (density-independent on both shells).
pub const DEFAULT_READING_MARGINS: u16 = 26;
/// `reading_margins` bounds; `set_settings` clamps rather than rejects.
pub const MIN_READING_MARGINS: u16 = 16;
pub const MAX_READING_MARGINS: u16 = 48;

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
    /// When the reminder fires, in minutes after local midnight
    /// (0..=[`MAX_REMINDER_MINUTES`]). Stored regardless of
    /// `evening_reminder`, so toggling the reminder off and on keeps the
    /// chosen hour.
    pub reminder_minutes: u16,
    /// Display name of the purely local account; empty means "not set".
    pub account_name: String,
    /// Contact email of the purely local account; empty means "not set".
    /// Stored as-is — there is no server to validate against.
    pub account_email: String,
    /// Opaque reading-font identifier; shells own the font roster.
    pub reading_font: String,
    /// Whether body text is forced to a heavier weight.
    pub reading_bold: bool,
    /// Line-height multiplier in
    /// [`MIN_LINE_SPACING`]..=[`MAX_LINE_SPACING`].
    pub line_spacing: f64,
    /// Extra letter spacing in em, 0.0..=[`MAX_LETTER_SPACING`].
    pub letter_spacing: f64,
    /// Extra word spacing in em, 0.0..=[`MAX_WORD_SPACING`].
    pub word_spacing: f64,
    /// Horizontal page margins in CSS px,
    /// [`MIN_READING_MARGINS`]..=[`MAX_READING_MARGINS`].
    pub reading_margins: u16,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            onboarded: false,
            reading_theme: DEFAULT_READING_THEME.to_string(),
            text_size_step: DEFAULT_TEXT_SIZE_STEP,
            brightness: DEFAULT_BRIGHTNESS,
            evening_reminder: false,
            reminder_minutes: DEFAULT_REMINDER_MINUTES,
            account_name: String::new(),
            account_email: String::new(),
            reading_font: DEFAULT_READING_FONT.to_string(),
            reading_bold: false,
            line_spacing: DEFAULT_LINE_SPACING,
            letter_spacing: 0.0,
            word_spacing: 0.0,
            reading_margins: DEFAULT_READING_MARGINS,
        }
    }
}
