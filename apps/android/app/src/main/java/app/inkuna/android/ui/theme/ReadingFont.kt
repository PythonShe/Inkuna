package app.inkuna.android.ui.theme

import androidx.annotation.StringRes
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import app.inkuna.android.R

/** The two engine faces offered by Customize. */
enum class ReadingFont(val id: String, @param:StringRes val nameRes: Int) {
    NOTO_SERIF("noto-serif", R.string.reader_font_noto_serif),
    NOTO_SANS("noto-sans", R.string.reader_font_noto_sans),
    ;

    companion object {
        val DEFAULT = NOTO_SERIF

        /** Normalizes historical shell-only font ids to the engine roster. */
        fun normalize(stored: String): ReadingFont = when (stored.trim().lowercase()) {
            NOTO_SANS.id, "system-sans" -> NOTO_SANS
            NOTO_SERIF.id -> NOTO_SERIF
            else -> NOTO_SERIF
        }

        fun from(id: String): ReadingFont = normalize(id)
    }
}

@Composable
fun ReadingFont.composeFamily(): FontFamily {
    val assets = LocalContext.current.assets
    return when (this) {
        ReadingFont.NOTO_SERIF -> remember(assets) {
            FontFamily(
                Font("fonts/NotoSerif.ttf", assets),
                Font("fonts/NotoSerif-Italic.ttf", assets, style = FontStyle.Italic),
            )
        }
        ReadingFont.NOTO_SANS -> remember(assets) {
            FontFamily(
                Font("fonts/NotoSans.ttf", assets),
                Font("fonts/NotoSans-Italic.ttf", assets, style = FontStyle.Italic),
            )
        }
    }
}
