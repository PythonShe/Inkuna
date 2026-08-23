package app.inkuna.android.ui.theme

import org.junit.Assert.assertEquals
import org.junit.Test

class ReadingFontTest {
    @Test
    fun normalizeMapsLegacySystemSansToNotoSans() {
        assertEquals(ReadingFont.NOTO_SANS, ReadingFont.normalize("system-sans"))
    }

    @Test
    fun normalizePreservesShippedFontIds() {
        assertEquals(ReadingFont.NOTO_SERIF, ReadingFont.normalize(ReadingFont.NOTO_SERIF.id))
        assertEquals(ReadingFont.NOTO_SANS, ReadingFont.normalize(ReadingFont.NOTO_SANS.id))
    }

    @Test
    fun normalizeDefaultsUnknownAndLegacyFacesToNotoSerif() {
        assertEquals(ReadingFont.NOTO_SERIF, ReadingFont.normalize("publisher"))
        assertEquals(ReadingFont.NOTO_SERIF, ReadingFont.normalize("anything-else"))
    }
}
