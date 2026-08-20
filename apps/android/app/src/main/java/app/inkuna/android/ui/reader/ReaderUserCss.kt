package app.inkuna.android.ui.reader

import app.inkuna.android.model.AppSettings
import app.inkuna.android.ui.theme.ReadingFont
import java.util.Locale

/**
 * The reader's typography as one value — the CSS pipeline's input, also
 * usable as an uncommitted draft while a slider is mid-drag.
 */
data class ReaderTypeDraft(
    val font: ReadingFont,
    val bold: Boolean,
    val lineSpacing: Float,
    val letterSpacing: Float,
    val wordSpacing: Float,
    val margins: Int,
) {
    companion object {
        fun from(snapshot: AppSettings.Snapshot) = ReaderTypeDraft(
            font = snapshot.readingFont,
            bold = snapshot.readingBold,
            lineSpacing = snapshot.lineSpacing,
            letterSpacing = snapshot.letterSpacing,
            wordSpacing = snapshot.wordSpacing,
            margins = snapshot.readingMargins,
        )
    }
}

/**
 * Renders a [ReaderTypeDraft] to the user stylesheet.
 *
 * Every rule is prefixed `:root[style]` — Readium always writes a style
 * attribute on `<html>`, so the selector always matches, and its (0,1,0)+
 * specificity beats ReadiumCSS's own `!important` rules (between two
 * `!important` declarations higher specificity wins, source order only
 * breaks ties). The `@font-face` sources point at the assets origin the
 * navigator serves with CORS enabled (`servedAssets` in
 * ReaderNavigatorHost); the block itself is ours and survives the
 * navigator.
 */
object ReaderUserCss {

    private const val FONTS_ORIGIN = "https://readium_assets/fonts"

    fun build(draft: ReaderTypeDraft): String = buildString {
        // Non-localized formatting: a de-locale device must not emit 1,65.
        fun num(value: Float) = String.format(Locale.ROOT, "%.4g", value).trimEnd('0').trimEnd('.')

        when (draft.font) {
            ReadingFont.NotoSerif -> appendFontFaces("Inkuna Noto Serif", "NotoSerif")
            ReadingFont.NotoSans -> appendFontFaces("Inkuna Noto Sans", "NotoSans")
            else -> Unit
        }

        // The values as custom properties: the forward contract the future
        // renderer (and any diagnostics) can read back off the page.
        append(":root[style]{--inkuna-line-height:${num(draft.lineSpacing)};")
        append("--inkuna-letter-spacing:${num(draft.letterSpacing)}em;")
        append("--inkuna-word-spacing:${num(draft.wordSpacing)}em;")
        append("--inkuna-margins:${draft.margins}px}")

        // Margins: body is the fragmented child of the :root multicol, so
        // inline-axis padding repeats inside every column and never moves
        // the column grid the pager measures (ReadiumCSS's own pageMargins
        // rule works the same way).
        append(
            ":root[style] body{padding-left:var(--inkuna-margins)!important;" +
                "padding-right:var(--inkuna-margins)!important}"
        )

        // Line height: root value + explicit inherit on the blocks a
        // publisher may have styled directly. Headings keep their own.
        append(":root[style]{line-height:var(--inkuna-line-height)!important}")
        append(
            ":root[style] body,:root[style] p,:root[style] li,:root[style] div," +
                ":root[style] dd,:root[style] blockquote{line-height:inherit!important}"
        )

        val spacedBlocks = listOf("h1", "h2", "h3", "h4", "h5", "h6", "p", "li", "div", "dd", "blockquote", "td")
            .joinToString(",") { ":root[style] $it" }
        if (draft.letterSpacing > 0f) {
            append(
                "$spacedBlocks{letter-spacing:var(--inkuna-letter-spacing)!important;" +
                    "font-variant:none!important}"
            )
        }
        if (draft.wordSpacing > 0f) {
            append("$spacedBlocks{word-spacing:var(--inkuna-word-spacing)!important}")
        }

        // Bold: body text only. b/strong/th/headings are left alone so the
        // publisher's own emphasis (700+) stays heavier than the text.
        if (draft.bold) {
            append(
                ":root[style] body,:root[style] p,:root[style] li,:root[style] div," +
                    ":root[style] dd,:root[style] blockquote,:root[style] td" +
                    "{font-weight:600!important}"
            )
        }

        // Font family: the ReadiumCSS-after selector set, including the
        // :not([lang]) guards that keep inline foreign-language runs on
        // their own face. Publisher emits nothing at all.
        draft.font.cssStack?.let { stack ->
            append(":root[style]{font-family:$stack!important}")
            append(
                ":root[style] body,:root[style] p,:root[style] li,:root[style] div," +
                    ":root[style] dt,:root[style] dd{font-family:inherit!important}"
            )
            val inlineRuns = listOf("i", "em", "cite", "b", "strong", "span")
                .joinToString(",") { ":root[style] $it:not([lang]):not([xml\\:lang])" }
            append("$inlineRuns{font-family:inherit!important}")
        }
    }

    private fun StringBuilder.appendFontFaces(family: String, file: String) {
        for ((style, suffix) in listOf("normal" to "", "italic" to "-Italic")) {
            append(
                "@font-face{font-family:\"$family\";font-style:$style;" +
                    "font-weight:100 900;src:url(\"$FONTS_ORIGIN/$file$suffix.ttf\")}"
            )
        }
    }
}
