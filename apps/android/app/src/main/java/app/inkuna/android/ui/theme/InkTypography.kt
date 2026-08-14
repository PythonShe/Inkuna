package app.inkuna.android.ui.theme

import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp

/**
 * Type scale from the Inkuna design system (tokens/typography.css).
 * The design pairs Newsreader (serif) + Instrument Sans; like iOS (which
 * substituted New York/SF), we use the platform families: system serif
 * falls back to Noto Serif CJK for Han text — CJK support is a core goal
 * and a bundled Latin serif would break that fallback chain.
 */
val InkSerif = FontFamily.Serif
val InkSans = FontFamily.Default

object InkType {
    /** 40sp hero/screen titles */
    val display = TextStyle(
        fontFamily = InkSerif,
        fontWeight = FontWeight.Light,
        fontSize = 40.sp,
        lineHeight = 46.sp,
    )

    /** 28sp section titles */
    val title = TextStyle(
        fontFamily = InkSerif,
        fontWeight = FontWeight.Normal,
        fontSize = 28.sp,
        lineHeight = 34.sp,
    )

    /** 22sp in-screen section titles (design 1.4rem) */
    val sectionTitle = TextStyle(
        fontFamily = InkSerif,
        fontWeight = FontWeight.Normal,
        fontSize = 22.sp,
        lineHeight = 28.sp,
    )

    /** 30sp book-detail title, stat numerals */
    val displaySmall = TextStyle(
        fontFamily = InkSerif,
        fontWeight = FontWeight.Light,
        fontSize = 30.sp,
        lineHeight = 36.sp,
    )

    /** 20sp card/book titles */
    val heading = TextStyle(
        fontFamily = InkSerif,
        fontWeight = FontWeight.Medium,
        fontSize = 20.sp,
        lineHeight = 26.sp,
    )

    /** 17sp book body; the reader recomputes size/line height per step */
    val reading = TextStyle(
        fontFamily = InkSerif,
        fontWeight = FontWeight.Normal,
        fontSize = 17.sp,
        lineHeight = 28.sp,
    )

    /** 15sp UI labels */
    val ui = TextStyle(
        fontFamily = InkSans,
        fontWeight = FontWeight.Medium,
        fontSize = 15.sp,
        lineHeight = 21.sp,
    )

    /** 13sp secondary UI */
    val label = TextStyle(
        fontFamily = InkSans,
        fontWeight = FontWeight.Medium,
        fontSize = 13.sp,
        lineHeight = 18.sp,
    )

    /** 12sp metadata */
    val caption = TextStyle(
        fontFamily = InkSans,
        fontWeight = FontWeight.Normal,
        fontSize = 12.sp,
        lineHeight = 16.sp,
    )

    /** Uppercase eyebrow labels: caption + wide tracking (--tracking-caps) */
    val eyebrow = caption.copy(letterSpacing = 0.08.em)
}
