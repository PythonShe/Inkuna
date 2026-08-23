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
