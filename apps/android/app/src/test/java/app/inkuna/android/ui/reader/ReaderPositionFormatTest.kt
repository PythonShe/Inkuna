package app.inkuna.android.ui.reader

import java.util.IllegalFormatConversionException
import java.util.Locale
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test

class ReaderPositionFormatTest {
    @Test
    fun convertsCoreUnsignedPositionsForAndroidPercentDResources() {
        assertEquals(listOf(3, 10), ReaderPositionFormat.resourceArgs(3u, 10u))
    }

    @Test
    fun convertsASingleCoreUnsignedPositionForAPercentDResource() {
        assertEquals(7, ReaderPositionFormat.resourceArg(7u))
    }

    /**
     * The in-book search panel crashed on every result row because it fed a
     * raw [UInt] to a `%d` resource. Kotlin boxes an unsigned value as
     * `kotlin.UInt`, which the formatter refuses — and in a composable that
     * throw escapes the Recomposer and takes the app down. This pins the
     * mechanism so the conversion is never quietly dropped again.
     */
    @Test
    fun rawUnsignedPositionIsRejectedByPercentDFormatting() {
        val chapterPage = "p. %1\$d"
        assertThrows(IllegalFormatConversionException::class.java) {
            String.format(Locale.US, chapterPage, 7u)
        }
        assertEquals("p. 7", String.format(Locale.US, chapterPage, ReaderPositionFormat.resourceArg(7u)))
    }
}
