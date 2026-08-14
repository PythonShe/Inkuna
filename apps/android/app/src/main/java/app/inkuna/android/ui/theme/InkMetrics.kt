package app.inkuna.android.ui.theme

import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.unit.dp

/** Spacing scale (tokens/spacing.css). */
object InkSpace {
    val s1 = 4.dp
    val s2 = 8.dp
    val s3 = 12.dp
    val s4 = 16.dp
    val s5 = 20.dp
    val s6 = 24.dp
    val s8 = 32.dp
    val s10 = 40.dp
    val s12 = 48.dp
    val s16 = 64.dp

    /** Screen edge gutter */
    val pageMargin = 20.dp
}

/** Radii (tokens/surface.css). Book covers stay rectangular: xs. */
object InkRadius {
    val xs = 4.dp
    val sm = 8.dp
    val md = 12.dp
    val lg = 16.dp
    val xl = 22.dp

    val xsShape = RoundedCornerShape(xs)
    val smShape = RoundedCornerShape(sm)
    val mdShape = RoundedCornerShape(md)
    val lgShape = RoundedCornerShape(lg)
    val xlShape = RoundedCornerShape(xl)
    val pillShape = RoundedCornerShape(percent = 50)
}

/** Motion (tokens/motion.css). Settles, never bounces. */
object InkMotion {
    /** Default fades, color changes */
    val easeQuiet = CubicBezierEasing(0.4f, 0f, 0.2f, 1f)

    /** Page turns, sheets */
    val easePage = CubicBezierEasing(0.22f, 1f, 0.36f, 1f)

    const val durFast = 180
    const val durMed = 320
    const val durSlow = 560
}
