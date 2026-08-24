package app.inkuna.android.ui.reader

import org.junit.Assert.assertEquals
import org.junit.Test

class ReaderPositionFormatTest {
    @Test
    fun convertsCoreUnsignedPositionsForAndroidPercentDResources() {
        assertEquals(listOf(3, 10), ReaderPositionFormat.resourceArgs(3u, 10u))
    }
}
