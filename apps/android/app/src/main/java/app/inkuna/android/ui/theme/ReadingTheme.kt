package app.inkuna.android.ui.theme

import androidx.annotation.StringRes
import androidx.compose.ui.graphics.Color
import app.inkuna.android.R

/**
 * The four reading surfaces from the Theme & type sheet. Picking a dark
 * reading theme (Quiet, Moon) also flips the surrounding app chrome to
 * night, and vice versa — one ritual, not two settings. Surface colors are
 * fixed, not day/night pairs: the reading surface IS the theme.
 */
enum class ReadingTheme(
    @param:StringRes val nameRes: Int,
    @param:StringRes val subtitleRes: Int,
    val background: Color,
    val foreground: Color,
    val isNight: Boolean,
) {
    Paper(R.string.theme_paper, R.string.theme_paper_subtitle, Color(0xFFF4EEE1), Color(0xFF2C261D), isNight = false),
    Calm(R.string.theme_calm, R.string.theme_calm_subtitle, Color(0xFFEDE3CE), Color(0xFF3A3226), isNight = false),
    Quiet(R.string.theme_quiet, R.string.theme_quiet_subtitle, Color(0xFF3B3833), Color(0xFFD8D2C6), isNight = true),
    Moon(R.string.theme_moon, R.string.theme_moon_subtitle, Color(0xFF1D1B16), Color(0xFFE2D9C6), isNight = true);

    /** Secondary ink on the reading surface (fg at 55%). */
    val dimmed: Color get() = foreground.copy(alpha = 0.55f)
}
