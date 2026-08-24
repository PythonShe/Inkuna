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
 * Stored by the core as an opaque id. [Publisher] keeps the book's own
 * embedded faces; the `system-*` ids select the platform faces registered
 * with the engine at startup; the `noto-*` ids pin the bundled Latin
 * variable cuts in `assets/fonts/`. CJK glyphs always fall through to the
 * bundled CJK Notos, which is a product requirement, never an omission.
 */
enum class ReadingFont(val id: String, @param:StringRes val nameRes: Int) {
    /**
     * The publication's own faces: the engine honors the book's
     * `@font-face` rules and falls back to Noto Serif.
     */
    Publisher("publisher", R.string.reader_font_publisher),
    SystemSerif("system-serif", R.string.reader_font_system_serif),
    SystemSans("system-sans", R.string.reader_font_system_sans),
    NotoSerif("noto-serif", R.string.reader_font_noto_serif),
    NotoSans("noto-sans", R.string.reader_font_noto_sans),
    ;

    companion object {
        /** The fresh-install face — mirrors the core DB default. */
        val DEFAULT = Publisher

        /**
         * Known ids map to themselves; anything else folds to [NotoSerif],
         * exactly as the engine folds unknown ids, so the shell's readout
         * and the laid-out page can never disagree.
         */
        fun normalize(stored: String): ReadingFont {
            val id = stored.trim().lowercase()
            return entries.firstOrNull { it.id == id } ?: NotoSerif
        }

        fun from(id: String): ReadingFont = normalize(id)
    }
}

/**
 * The face the "Aa" specimens and the Font menu items render in. UI-only:
 * the reading surface gets its faces from the engine's registry.
 * [ReadingFont.Publisher] has no knowable face here and stands in with the
 * serif reading face. Deliberately NOT part of [InkSerif]/[InkSans]: a
 * bundled Latin face in the app-wide fallback chain would break CJK UI text.
 */
@Composable
fun ReadingFont.composeFamily(): FontFamily {
    val assets = LocalContext.current.assets
    return when (this) {
        ReadingFont.Publisher, ReadingFont.SystemSerif -> FontFamily.Serif
        ReadingFont.SystemSans -> FontFamily.SansSerif
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
