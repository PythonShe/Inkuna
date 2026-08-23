package app.inkuna.android.ui.theme

import org.junit.Assert.assertEquals
import org.junit.Test

class ReadingFontTest {
    @Test
    fun normalizeMapsLegacySystemSansToNotoSans() {
        assertEquals(ReadingFont.NOTO_SANS, ReadingFont.normalize("system-sans"))
    }

    @Test
    fun normalizeDefaultsUnknownAndLegacyFacesToNotoSerif() {
        assertEquals(ReadingFont.NOTO_SERIF, ReadingFont.normalize("publisher"))
        assertEquals(ReadingFont.NOTO_SERIF, ReadingFont.normalize("anything-else"))
    }
}
