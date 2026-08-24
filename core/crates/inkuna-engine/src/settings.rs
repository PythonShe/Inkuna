//! Reader layout settings: the engine-side value of the shells'
//! Customize panel, plus the resolved typography numbers layout consumes.
//!
//! The concrete numbers are TRANSCRIBED, not invented — recorded
//! 2026-08-22 from the shells before their plan-02 deletion, because the
//! deleted files must never be the only record of the current look:
//!
//! - step→size table: `apps/ios/Inkuna/DesignSystem/Tokens/TypographyTokens.swift`
//!   (`ReadingTextSize.pointSize`) and
//!   `apps/android/app/src/main/java/app/inkuna/android/model/AppSettings.kt`
//!   (`TEXT_SIZE_STEPS`) — the shells agree exactly.
//! - spacing/margin ranges (the shells' WebView era rendered the bold
//!   toggle at weight 600; the engine deliberately maps it to 700 so
//!   Latin and the static CJK/Hebrew Bold faces agree — see
//!   `Typography::bold_base`):
//!   `apps/ios/Inkuna/Reader/ReaderUserStyle.swift`,
//!   `apps/ios/Inkuna/Model/AppSettings.swift`, and
//!   `apps/android/app/src/main/java/app/inkuna/android/ui/reader/ReaderUserCss.kt` —
//!   again identical across shells.
//! - font roster ids: `apps/ios/Inkuna/DesignSystem/ReadingFont.swift` /
//!   `.../ui/theme/ReadingFont.kt`.

/// Body-text size per `text_size_step`, in layout points. Transcribed —
/// see the module doc; both shells carry exactly this five-step table.
pub const TEXT_SIZE_STEPS_PT: [f64; 5] = [14.4, 15.6, 17.0, 18.4, 20.0];

/// The shells' slider bounds, applied by [`LayoutSettings::clamped`].
const LINE_SPACING_RANGE: (f64, f64) = (1.30, 2.10);
const LETTER_SPACING_RANGE: (f64, f64) = (0.0, 0.06);
const WORD_SPACING_RANGE: (f64, f64) = (0.0, 0.30);
const MARGINS_RANGE: (u32, u32) = (16, 48);
const DEFAULT_LINE_SPACING: f64 = 1.65;

/// Space between paragraphs as a multiple of the body size.
/// engine-chosen: no prior renderer precedent in either shell — this mirrors
/// the `p { margin: 1em 0 }` WebView UA default previously used by the shell.
const PARAGRAPH_SPACING_EM: f64 = 1.0;
/// First-line indent in points. engine-chosen: no prior renderer precedent —
/// neither shell indents; publishers indent via their own CSS.
const PARAGRAPH_INDENT_PT: f64 = 0.0;
/// h1..h6 size multipliers over the body size. engine-chosen: no prior renderer
/// precedent in either shell — mirrors the WebView UA defaults the
/// former shell renderer used.
const HEADING_SCALE: [f64; 6] = [2.0, 1.5, 1.17, 1.0, 0.83, 0.67];
/// Ruby annotation size over base size, as an exact ratio (M4 shapes ruby
/// at `size.mul_ratio(num, den)`). engine-chosen: no prior renderer precedent.
const RUBY_SCALE: (u32, u32) = (1, 2);

/// The reading-face family a roster id resolves to. `Publisher` and the
/// two `System*` values are *requests*, not guarantees: the registry
/// serves them from its dynamically registered faces and silently falls
/// back to the bundled Noto equivalent when none were registered
/// (publisher faces are the book's own embedded fonts, extracted and
/// registered per session; system faces are whatever the shell
/// registered before the first reader open).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontFamily {
    /// The book's own embedded faces: element `font-family` stacks pick
    /// among the registered publisher families (`select_stack`); a stack
    /// that matches nothing, a book with no embedded fonts, and elements
    /// with no stack fall back to [`FontFamily::NotoSerif`] (a generic
    /// `sans-serif` in the stack to [`FontFamily::NotoSans`]).
    Publisher,
    /// The platform's serif face set, as registered by the shell.
    SystemSerif,
    /// The platform's sans face set, as registered by the shell.
    SystemSans,
    NotoSerif,
    NotoSans,
}

impl FontFamily {
    /// Whether the family is serif-flavored — what the CJK/Hebrew
    /// fallback stages key their serif/sans split on for the
    /// settings-owned families. `Publisher` defaults to serif (its
    /// terminal fallback is NotoSerif), but under a `font-family`
    /// stack the resolved reading chain overrides this with the
    /// stack's generic keyword — `…, sans-serif` gets the sans
    /// CJK/Hebrew Notos (see `FontRegistry::reading_chain`).
    pub fn is_serif(self) -> bool {
        !matches!(self, FontFamily::SystemSans | FontFamily::NotoSans)
    }
}

/// The seven Customize settings, as the shells persist them. Apply
/// [`LayoutSettings::clamped`] at every engine entry; out-of-range values
/// clamp and unknown fonts default, never error.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutSettings {
    /// Roster font id, opaque (`"noto-serif"`, `"publisher"`, …).
    pub reading_font: String,
    pub reading_bold: bool,
    /// Index into [`TEXT_SIZE_STEPS_PT`], 0..=4, clamped.
    pub text_size_step: u8,
    /// Line height over body size, 1.30..=2.10, clamped.
    pub line_spacing: f64,
    /// Extra letter spacing in em, 0.0..=0.06, clamped.
    pub letter_spacing: f64,
    /// Extra word spacing in em, 0.0..=0.30, clamped.
    pub word_spacing: f64,
    /// Inline-axis page margins in layout points (reinterpreted from the
    /// shells' px — numerically identical at 1×), 16..=48, clamped.
    pub reading_margins: u32,
}

impl Default for LayoutSettings {
    /// The shells' shared defaults (both `AppSettings` files).
    fn default() -> Self {
        Self {
            reading_font: "publisher".to_string(),
            reading_bold: false,
            text_size_step: 2,
            line_spacing: DEFAULT_LINE_SPACING,
            letter_spacing: 0.0,
            word_spacing: 0.0,
            reading_margins: 26,
        }
    }
}

impl LayoutSettings {
    /// Every field forced into its documented range; non-finite floats
    /// fall back to their defaults rather than poisoning layout.
    pub fn clamped(self) -> Self {
        Self {
            reading_font: self.reading_font,
            reading_bold: self.reading_bold,
            text_size_step: self
                .text_size_step
                .min((TEXT_SIZE_STEPS_PT.len() - 1) as u8),
            line_spacing: clamp_finite(self.line_spacing, LINE_SPACING_RANGE, DEFAULT_LINE_SPACING),
            letter_spacing: clamp_finite(self.letter_spacing, LETTER_SPACING_RANGE, 0.0),
            word_spacing: clamp_finite(self.word_spacing, WORD_SPACING_RANGE, 0.0),
            reading_margins: self.reading_margins.clamp(MARGINS_RANGE.0, MARGINS_RANGE.1),
        }
    }

    /// A 64-bit digest of the settings as layout consumes them: the
    /// CLAMPED values are hashed, so two settings that clamp identically
    /// fingerprint identically (they lay out identically), and any
    /// effective field change rehashes. First 8 bytes (LE) of blake3 over
    /// a canonical encoding: fields in declaration order, strings as
    /// `len (u64 LE) + bytes`, `f64` as `to_bits()` LE.
    pub fn fingerprint(&self) -> u64 {
        let clamped = self.clone().clamped();
        let mut hasher = blake3::Hasher::new();
        hasher.update(&(clamped.reading_font.len() as u64).to_le_bytes());
        hasher.update(clamped.reading_font.as_bytes());
        hasher.update(&[u8::from(clamped.reading_bold)]);
        hasher.update(&[clamped.text_size_step]);
        hasher.update(&clamped.line_spacing.to_bits().to_le_bytes());
        hasher.update(&clamped.letter_spacing.to_bits().to_le_bytes());
        hasher.update(&clamped.word_spacing.to_bits().to_le_bytes());
        hasher.update(&clamped.reading_margins.to_le_bytes());
        let digest = hasher.finalize();
        let mut first = [0u8; 8];
        first.copy_from_slice(&digest.as_bytes()[..8]);
        u64::from_le_bytes(first)
    }

    /// The requested reading family for the roster id, one arm per
    /// roster value. Unknown ids (a stale persisted value, a future
    /// roster entry) fall back to NotoSerif — the registry then resolves
    /// `Publisher`/`System*` requests against its registered faces,
    /// falling back to the bundled Notos when none exist.
    pub fn font_family(&self) -> FontFamily {
        match self.reading_font.to_ascii_lowercase().as_str() {
            "publisher" => FontFamily::Publisher,
            "system-serif" => FontFamily::SystemSerif,
            "system-sans" => FontFamily::SystemSans,
            "noto-sans" => FontFamily::NotoSans,
            // "noto-serif" and every unknown id.
            _ => FontFamily::NotoSerif,
        }
    }

    /// The resolved typography numbers. `f64` layout points until the
    /// fixed-point module lands in M3; M4 converts at consumption.
    pub fn typography(&self) -> Typography {
        let clamped = self.clone().clamped();
        let font_size = TEXT_SIZE_STEPS_PT[clamped.text_size_step as usize];
        Typography {
            font_size,
            line_height: font_size * clamped.line_spacing,
            paragraph_spacing: font_size * PARAGRAPH_SPACING_EM,
            paragraph_indent: PARAGRAPH_INDENT_PT,
            heading_scale: HEADING_SCALE,
            ruby_scale: RUBY_SCALE,
            bold_base: clamped.reading_bold,
        }
    }
}

/// The numbers layout actually consumes, all in layout points.
#[derive(Debug, Clone, PartialEq)]
pub struct Typography {
    /// Body size.
    pub font_size: f64,
    /// Body line height (size × line_spacing).
    pub line_height: f64,
    pub paragraph_spacing: f64,
    pub paragraph_indent: f64,
    /// h1..h6 multiplier over the body size.
    pub heading_scale: [f64; 6],
    /// Ruby size over base size as an exact ratio, e.g. `(1, 2)`.
    pub ruby_scale: (u32, u32),
    /// The reader's bold toggle: body text renders at weight 700 —
    /// CSS `bold`, matching the static CJK/Hebrew Bold faces so one
    /// toggled paragraph never mixes weights across scripts — while
    /// publisher emphasis heavier than 700 stays heavier.
    pub bold_base: bool,
}

/// Clamp that treats NaN/inf as "never set": the default, not a poison.
fn clamp_finite(value: f64, (min, max): (f64, f64), default: f64) -> f64 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        default
    }
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
