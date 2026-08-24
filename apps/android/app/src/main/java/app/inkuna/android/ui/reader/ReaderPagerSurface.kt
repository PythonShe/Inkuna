package app.inkuna.android.ui.reader

/**
 * The renderer-shaped strip contract shared by the Android reader pagers.
 *
 * Offsets are view pixels: the pager consumes touch coordinates, while page
 * display-list geometry remains in layout points inside [engine.PageView].
 */
interface ReaderPagerSurface {
    val isEngageable: Boolean
    val isBusy: Boolean
    val hasActiveSelection: Boolean
    val isRightToLeft: Boolean

    /**
     * Brackets one paging interaction (a drag, or a programmatic turn's
     * settle). While bracketed the surface freezes the strip's page count
     * so a mid-gesture layout event cannot rebase offsets under the
     * finger; the deferred rebase lands in [endPagingInteraction].
     * Mirrors the iOS `ReaderPagerSurface` members of the same names.
     */
    fun beginPagingInteraction()
    fun endPagingInteraction()

    fun innerMetrics(): ReaderPagerStrip?
    fun setInnerOffset(x: Float)
    fun outerMetrics(): ReaderPagerStrip?
    fun setOuterOffset(x: Float)
    fun neighborIsReady(toRight: Boolean): Boolean
    fun commitBoundaryCrossing(toRight: Boolean): Boolean
}

data class ReaderPagerStrip(
    val offset: Float,
    val range: ClosedFloatingPointRange<Float>,
    val pageWidth: Float,
)
