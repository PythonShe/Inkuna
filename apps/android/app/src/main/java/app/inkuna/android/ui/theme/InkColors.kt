package app.inkuna.android.ui.theme

import androidx.compose.runtime.Immutable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.ui.graphics.Color

/**
 * Semantic color roles from the Inkuna design system (tokens/colors.css).
 * Two fixed palettes — Paper (day) and Moon (night) — selected by the app
 * appearance, not by wallpaper-derived dynamic color: the brand is the ink.
 */
@Immutable
data class InkColors(
    val bgApp: Color,
    val bgSurface: Color,
    val bgRaised: Color,
    val bgReading: Color,
    val bgRecessed: Color,
    val textDisplay: Color,
    val textBody: Color,
    val textSecondary: Color,
    val textTertiary: Color,
    val accent: Color,
    val accentFill: Color,
    val accentText: Color,
    val accentInk: Color,
    val accentSoft: Color,
    val moon: Color,
    val positive: Color,
    val danger: Color,
    val borderHairline: Color,
    val scrim: Color,
    val focusRing: Color,
    val isNight: Boolean,
)

val InkDayColors = InkColors(
    bgApp = Color(0xFFF6F1E8),        // paper-1
    bgSurface = Color(0xFFFBF8F2),    // paper-0
    bgRaised = Color(0xFFFFFEFA),
    bgReading = Color(0xFFF4EEE1),
    bgRecessed = Color(0xFFEFE7D9),   // paper-2
    textDisplay = Color(0xFF241F17),  // ink-1
    textBody = Color(0xFF2C261D),
    // ink-2, darkened from the design's #6B6255 to buy back the gap that
    // raising ink-3 to the AA floor closed: the two were L+0.014 apart, which
    // renders as one level. Day paper has less headroom above the floor than
    // night, so the separation has to come out of secondary.
    textSecondary = Color(0xFF60584C),
    // ink-3, darkened from the design's #9A9083: at 2.6:1 it fell under the
    // legibility floor for captions, page numbers and search placeholders.
    // Measured on bgRecessed (4.52) — the darkest ground, not the kindest.
    textTertiary = Color(0xFF70675B),
    // Three accent roles, because one amber cannot be both a fill and an ink
    // on cream paper. A filled surface only has to be dark enough to carry
    // its own label, and sitting near that floor is what keeps it reading
    // amber rather than brown; a glyph is thin strokes on pale ground and has
    // to go darker still.
    accent = Color(0xFFB4863B),        // amber — brand mark, fills with no label
    accentFill = Color(0xFF8F6829),    // --amber-deep; 4.8:1 under accentInk
    accentText = Color(0xFF7F5A1E),    // 4.5:1+ on every day ground
    accentInk = Color(0xFFFBF8F2),
    accentSoft = Color(0x1FB4863B),    // amber 12%
    moon = Color(0xFF8E97A6),
    positive = Color(0xFF557153),
    danger = Color(0xFF9C4A38),
    borderHairline = Color(0x17241F17), // ink-1 9%
    scrim = Color(0x6618140E),          // 40%
    focusRing = Color(0x59B4863B),      // amber 35%
    isNight = false,
)

val InkNightColors = InkColors(
    bgApp = Color(0xFF191713),
    bgSurface = Color(0xFF211E18),
    bgRaised = Color(0xFF2A261F),
    bgReading = Color(0xFF1D1B16),
    bgRecessed = Color(0xFF14120E),
    textDisplay = Color(0xFFEEE6D6),
    textBody = Color(0xFFE2D9C6),
    textSecondary = Color(0xFFA89D8C),
    // Lightened to clear AA on bgRaised (4.52), the lightest night ground.
    textTertiary = Color(0xFF948C7D),
    // Lamplight amber on near-black: fill and ink want the same value here,
    // so all three roles collapse and night is unaffected by the day split.
    accent = Color(0xFFD9AE63),
    accentFill = Color(0xFFD9AE63),
    accentText = Color(0xFFD9AE63),
    accentInk = Color(0xFF241F17),
    accentSoft = Color(0x24D9AE63),     // amber 14%
    moon = Color(0xFFA9B2C0),
    positive = Color(0xFF7E9B7B),
    danger = Color(0xFFC07660),
    borderHairline = Color(0x17EEE6D6), // 9%
    scrim = Color(0x8C0A0907),          // 55%
    focusRing = Color(0x59D9AE63),      // 35%
    isNight = true,
)

/**
 * Deliberately not `staticCompositionLocalOf`: the day/night cross-fade
 * rewrites this value once per frame, and a static local would recompose the
 * entire app tree each time instead of only the readers.
 */
val LocalInkColors = compositionLocalOf { InkDayColors }
