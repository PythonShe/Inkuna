package app.inkuna.android.ui.theme

import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf
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
    textSecondary = Color(0xFF6B6255), // ink-2
    textTertiary = Color(0xFF9A9083),  // ink-3
    accent = Color(0xFFB4863B),        // amber
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
    textTertiary = Color(0xFF776F62),
    accent = Color(0xFFD9AE63),
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

val LocalInkColors = staticCompositionLocalOf { InkDayColors }
