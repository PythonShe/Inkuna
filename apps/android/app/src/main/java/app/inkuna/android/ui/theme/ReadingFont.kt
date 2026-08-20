package app.inkuna.android.ui.theme

import androidx.annotation.StringRes
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import app.inkuna.android.R

/**
 * The reader's font roster (the Font list in the Customize panel).
 *
 * Stored by the core as an opaque id — like the reading theme — so fonts
 * can ship shell-first. Bundled faces are the Noto Latin variable cuts in
 * `assets/fonts/`; CJK glyphs always fall through to the system faces,
 * which is a product requirement, never an omission.
 */
enum class ReadingFont(val id: String, @param:StringRes val nameRes: Int) {
    /** The publication's own faces: no font-family rule is emitted. */
    Publisher("publisher", R.string.reader_font_publisher),
    SystemSerif("system-serif", R.string.reader_font_system_serif),
    SystemSans("system-sans", R.string.reader_font_system_sans),
    NotoSerif("noto-serif", R.string.reader_font_noto_serif),
    NotoSans("noto-sans", R.string.reader_font_noto_sans),
    ;

    /**
     * The CSS font-family stack injected into the page, or null for
     * [Publisher]. Bundled families are namespaced "Inkuna Noto …"
     * because a publisher may embed a face literally named "Noto Serif";
     * the trailing generic keeps per-glyph CJK fallback intact.
     */
    val cssStack: String?
        get() = when (this) {
            Publisher -> null
            SystemSerif -> "serif"
            SystemSans -> "sans-serif"
            NotoSerif -> "\"Inkuna Noto Serif\",serif"
            NotoSans -> "\"Inkuna Noto Sans\",sans-serif"
        }

    companion object {
        val DEFAULT = Publisher

        /** Case-insensitive parse; unknown ids render as the default. */
        fun from(id: String): ReadingFont =
            entries.firstOrNull { it.id.equals(id, ignoreCase = true) } ?: DEFAULT
    }
}

/**
 * The face the "Aa" specimens and the preview card render in — loaded from
 * the same asset files the WebView is served, so specimen and page can
 * never disagree. Deliberately NOT part of [InkSerif]/[InkSans]: a bundled
 * Latin face in the app-wide fallback chain would break CJK UI text.
 */
@Composable
fun ReadingFont.composeFamily(): FontFamily {
    val assets = LocalContext.current.assets
    return when (this) {
        ReadingFont.Publisher, ReadingFont.SystemSerif -> InkSerif
        ReadingFont.SystemSans -> InkSans
        ReadingFont.NotoSerif -> remember(assets) {
            FontFamily(
                Font("fonts/NotoSerif.ttf", assets),
                Font("fonts/NotoSerif-Italic.ttf", assets, style = FontStyle.Italic),
            )
        }
        ReadingFont.NotoSans -> remember(assets) {
            FontFamily(
                Font("fonts/NotoSans.ttf", assets),
                Font("fonts/NotoSans-Italic.ttf", assets, style = FontStyle.Italic),
            )
        }
    }
}
