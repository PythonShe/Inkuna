package app.inkuna.android.ui.reader

import app.inkuna.android.model.AppSettings
import app.inkuna.android.ui.theme.ReadingFont
import app.inkuna.core.ReaderLayoutSettings

/** The editable state used by the appearance sheet's native preview. */
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

/** The one-to-one shell setting mapping the engine accepts for a relayout. */
fun AppSettings.Snapshot.readerLayoutSettings() = ReaderLayoutSettings(
    readingFont = ReadingFont.normalize(rawReadingFont).id,
    readingBold = readingBold,
    textSizeStep = textSizeStep.toUByte(),
    lineSpacing = lineSpacing.toDouble(),
    letterSpacing = letterSpacing.toDouble(),
    wordSpacing = wordSpacing.toDouble(),
    readingMargins = readingMargins.toUInt(),
)
