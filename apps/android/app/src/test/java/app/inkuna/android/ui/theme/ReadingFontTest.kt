package app.inkuna.android.ui.theme

import org.junit.Assert.assertEquals
import org.junit.Test

class ReadingFontTest {
    @Test
    fun normalizePreservesEveryRosterId() {
        ReadingFont.entries.forEach { font ->
            assertEquals(font, ReadingFont.normalize(font.id))
        }
    }

    @Test
    fun normalizeIgnoresCaseAndWhitespace() {
        assertEquals(ReadingFont.SystemSans, ReadingFont.normalize("  System-Sans\n"))
        assertEquals(ReadingFont.Publisher, ReadingFont.normalize("PUBLISHER"))
    }

    @Test
    fun normalizeFoldsUnknownIdsToNotoSerifLikeTheEngine() {
        assertEquals(ReadingFont.NotoSerif, ReadingFont.normalize("anything-else"))
        assertEquals(ReadingFont.NotoSerif, ReadingFont.normalize(""))
    }

    @Test
    fun defaultMatchesTheCoreDbDefault() {
        assertEquals("publisher", ReadingFont.DEFAULT.id)
    }
}
