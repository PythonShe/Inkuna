package app.inkuna.android.ui.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.getValue

/**
 * Inkuna's own token set rides in [LocalInkColors]; a matching Material 3
 * color scheme is derived from it so platform components (sheets, ripples,
 * text selection) stay on-palette.
 */
object InkTheme {
    val colors: InkColors
        @Composable @ReadOnlyComposable get() = LocalInkColors.current
}

private fun lerp(a: InkColors, b: InkColors, t: Float): InkColors {
    if (t <= 0f) return a
    if (t >= 1f) return b
    fun c(x: androidx.compose.ui.graphics.Color, y: androidx.compose.ui.graphics.Color) =
        androidx.compose.ui.graphics.lerp(x, y, t)
    return InkColors(
        bgApp = c(a.bgApp, b.bgApp),
        bgSurface = c(a.bgSurface, b.bgSurface),
        bgRaised = c(a.bgRaised, b.bgRaised),
        bgReading = c(a.bgReading, b.bgReading),
        bgRecessed = c(a.bgRecessed, b.bgRecessed),
        textDisplay = c(a.textDisplay, b.textDisplay),
        textBody = c(a.textBody, b.textBody),
        textSecondary = c(a.textSecondary, b.textSecondary),
        textTertiary = c(a.textTertiary, b.textTertiary),
        accent = c(a.accent, b.accent),
        accentFill = c(a.accentFill, b.accentFill),
        accentText = c(a.accentText, b.accentText),
        accentInk = c(a.accentInk, b.accentInk),
        accentSoft = c(a.accentSoft, b.accentSoft),
        moon = c(a.moon, b.moon),
        positive = c(a.positive, b.positive),
        danger = c(a.danger, b.danger),
        borderHairline = c(a.borderHairline, b.borderHairline),
        scrim = c(a.scrim, b.scrim),
        focusRing = c(a.focusRing, b.focusRing),
        isNight = b.isNight,
    )
}

@Composable
fun InkTheme(night: Boolean, content: @Composable () -> Unit) {
    // Cross-fade the whole palette on day/night flips, as iOS cross-fades
    // the window; interaction stays live during the transition.
    val fraction by androidx.compose.animation.core.animateFloatAsState(
        targetValue = if (night) 1f else 0f,
        animationSpec = androidx.compose.animation.core.tween(InkMotion.durMed, easing = InkMotion.easeQuiet),
        label = "dayNight",
    )
    val ink = lerp(InkDayColors, InkNightColors, fraction).let {
        if (night) it.copy(isNight = true) else it.copy(isNight = false)
    }
    val scheme = if (night) {
        darkColorScheme(
            primary = ink.accentFill,
            onPrimary = ink.accentInk,
            secondaryContainer = ink.accentSoft,
            onSecondaryContainer = ink.accentText,
            background = ink.bgApp,
            onBackground = ink.textBody,
            surface = ink.bgSurface,
            onSurface = ink.textBody,
            surfaceVariant = ink.bgRecessed,
            onSurfaceVariant = ink.textSecondary,
            surfaceContainer = ink.bgSurface,
            surfaceContainerLow = ink.bgSurface,
            surfaceContainerHigh = ink.bgRaised,
            surfaceContainerHighest = ink.bgRaised,
            outline = ink.textTertiary,
            outlineVariant = ink.borderHairline,
            scrim = ink.scrim,
            error = ink.danger,
        )
    } else {
        lightColorScheme(
            primary = ink.accentFill,
            onPrimary = ink.accentInk,
            secondaryContainer = ink.accentSoft,
            onSecondaryContainer = ink.accentText,
            background = ink.bgApp,
            onBackground = ink.textBody,
            surface = ink.bgSurface,
            onSurface = ink.textBody,
            surfaceVariant = ink.bgRecessed,
            onSurfaceVariant = ink.textSecondary,
            surfaceContainer = ink.bgSurface,
            surfaceContainerLow = ink.bgSurface,
            surfaceContainerHigh = ink.bgRaised,
            surfaceContainerHighest = ink.bgRaised,
            outline = ink.textTertiary,
            outlineVariant = ink.borderHairline,
            scrim = ink.scrim,
            error = ink.danger,
        )
    }
    val typography = Typography(
        displaySmall = InkType.display,
        headlineMedium = InkType.title,
        titleMedium = InkType.heading,
        bodyLarge = InkType.reading,
        bodyMedium = InkType.ui,
        labelLarge = InkType.ui,
        labelMedium = InkType.label,
        labelSmall = InkType.caption,
        bodySmall = InkType.caption,
    )
    CompositionLocalProvider(LocalInkColors provides ink) {
        MaterialTheme(colorScheme = scheme, typography = typography, content = content)
    }
}
