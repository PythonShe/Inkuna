package app.inkuna.android.ui.reader

import android.graphics.Rect
import android.view.View
import android.view.ViewGroup
import android.webkit.WebView
import androidx.viewpager.widget.ViewPager
import kotlin.math.roundToInt
import org.readium.r2.navigator.epub.EpubNavigatorFragment
import org.readium.r2.navigator.preferences.ReadingProgression
import org.readium.r2.shared.ExperimentalReadiumApi

/**
 * Transitional Readium implementation of [ReaderPagerSurface].
 *
 * This class deliberately contains every WebView/ViewPager workaround from
 * the old pager. Movement 5 removes it with Readium instead of allowing any
 * renderer knowledge to leak back into [ReaderPagerLayout].
 */
@OptIn(ExperimentalReadiumApi::class)
class ReadiumPagerSurface(
    private val navigator: EpubNavigatorFragment,
    private val hostView: View,
) : ReaderPagerSurface {
    private var webView: WebView? = null
    private var pager: ViewPager? = null
    private var innerPitch = 0f
    private var pitchSource: WebView? = null
    private var innerMax = Int.MAX_VALUE
    private var innerMaxGeneration = 0
    private var basePagerScrollX = 0
    private var outerOffset = 0f
    private var preRastered: WebView? = null
    private var fakeDragArmNeeded = false

    override val isEngageable: Boolean
        get() {
            if (navigator.overflow.value.scroll) return false
            val root = navigator.view ?: return false
            return visibleWebView(root)?.width ?: 0 > 0
        }

    override val isBusy: Boolean = false
    override val hasActiveSelection: Boolean
        get() = SelectionModeTracker.active
    override val isRightToLeft: Boolean
        get() = navigator.overflow.value.readingProgression == ReadingProgression.RTL

    override fun innerMetrics(): ReaderPagerStrip? {
        val root = navigator.view ?: return null
        val visible = visibleWebView(root) ?: return null
        if (visible.width <= 0) return null
        webView = visible
        seedInnerMax(visible)
        val pitch = innerPitch
        if (pitch <= 0f) return null
        val current = visible.scrollX.toFloat()
        val upper = when (innerMax) {
            Int.MAX_VALUE -> ((current / pitch).roundToInt() + 1) * pitch
            else -> innerMax.toFloat()
        }
        return ReaderPagerStrip(current, 0f..upper, pitch)
    }

    override fun setInnerOffset(x: Float) {
        val visible = webView ?: navigator.view?.let(::visibleWebView) ?: return
        val target = x.roundToInt()
        if (visible.scrollX != target) visible.scrollTo(target, visible.scrollY)
    }

    override fun outerMetrics(): ReaderPagerStrip? {
        val root = navigator.view ?: return null
        val current = pagerIn(root) ?: return null
        pager = current
        val width = hostView.width.toFloat()
        if (width <= 0f) return null
        val left = current.canScrollHorizontally(-1)
        val right = current.canScrollHorizontally(1)
        val home = width
        if (outerOffset == 0f) outerOffset = home
        return ReaderPagerStrip(
            offset = outerOffset,
            range = (if (left) 0f else width)..(if (right) width * 2f else width),
            pageWidth = width,
        )
    }

    override fun setOuterOffset(x: Float) {
        val metrics = outerMetrics() ?: return
        val home = metrics.pageWidth
        outerOffset = x
        val displacement = x - home
        val currentPager = pager
        if (currentPager?.isFakeDragging == true && displacement != 0f) {
            drivePager(displacement.coerceIn(-metrics.pageWidth, metrics.pageWidth))
            setChildTranslationX(0f)
            return
        }

        if (currentPager?.isFakeDragging == true && displacement == 0f) {
            closeFakeDrag()
        }
        // Outside the outer strip is the pager's rubber-band. The temporary
        // host owns that transform because only Readium has a child tree to
        // move; the engine canvas implements the same offset directly.
        setChildTranslationX(home - x)
    }

    override fun neighborIsReady(toRight: Boolean): Boolean {
        val root = navigator.view ?: return false
        val visible = webView ?: visibleWebView(root) ?: return false
        val currentPager = pager ?: pagerIn(root) ?: return false
        pager = currentPager
        val sign = if (toRight) 1 else -1
        if (!currentPager.canScrollHorizontally(sign)) return false
        val neighbor = neighbourWebView(root, visible, towardRight = toRight)
        if (neighbor == null || neighbor.progress < 100 || neighbor.contentHeight == 0) return false
        if (!currentPager.isFakeDragging) {
            if (!currentPager.beginFakeDrag()) return false
            basePagerScrollX = currentPager.scrollX
            fakeDragArmNeeded = false
        }
        prePositionNeighbour(neighbor, sign)
        return true
    }

    override fun commitBoundaryCrossing(toRight: Boolean): Boolean {
        val currentPager = pager ?: return false
        val committed = preRastered
        val positionedX = committed?.scrollX ?: 0
        if (currentPager.isFakeDragging) {
            runCatching { currentPager.endFakeDrag() }.getOrElse { return false }
        } else {
            return false
        }
        // Readium's fake-drag commit re-seats RTL resources at physical
        // column zero. Keep the pre-positioned logical entry page visible.
        if (committed != null && committed.scrollX != positionedX) {
            committed.scrollTo(positionedX, committed.scrollY)
        }
        preRastered?.settings?.offscreenPreRaster = false
        preRastered = null
        fakeDragArmNeeded = false
        outerOffset = hostView.width.toFloat()
        setChildTranslationX(0f)
        return true
    }

    /** The current Readium page for the type-style bridge that dies in Movement 5. */
    fun currentWebView(): WebView? = navigator.view?.let(::visibleWebView)

    /** Drops Readium's reflow-sensitive column geometry. */
    fun recalibrate() {
        closeFakeDrag()
        pitchSource = null
        innerPitch = 0f
        innerMax = Int.MAX_VALUE
        innerMaxGeneration += 1
        webView = null
    }

    private fun seedInnerMax(target: WebView) {
        innerMax = if (!target.canScrollHorizontally(1)) target.scrollX else Int.MAX_VALUE
        if (pitchSource !== target) {
            innerPitch = target.width.toFloat()
            pitchSource = target
        }
        val generation = ++innerMaxGeneration
        target.evaluateJavascript(
            "(function(){var d=document.scrollingElement;" +
                "var m=Math.max(0,Math.round((d.scrollWidth-d.clientWidth)*window.devicePixelRatio));" +
                "var c=Math.max(1,Math.round(d.scrollWidth/d.clientWidth));" +
                "return m+'|'+c;})()",
        ) { result ->
            if (generation != innerMaxGeneration || target !== webView) return@evaluateJavascript
            val parts = result?.trim('\"', ' ')?.split('|') ?: return@evaluateJavascript
            val measured = parts.getOrNull(0)?.toIntOrNull() ?: return@evaluateJavascript
            val columns = parts.getOrNull(1)?.toIntOrNull() ?: return@evaluateJavascript
            if (innerMax == Int.MAX_VALUE) innerMax = measured
            if (columns > 1 && innerMax in 1 until Int.MAX_VALUE) {
                innerPitch = innerMax.toFloat() / (columns - 1)
            }
        }
    }

    private fun drivePager(displacement: Float) {
        val currentPager = pager ?: return
        if (!currentPager.isFakeDragging) return
        rearmFakeDrag(currentPager)
        val target = basePagerScrollX + displacement
        val delta = currentPager.scrollX - target
        if (delta != 0f) currentPager.fakeDragBy(delta)
    }

    private fun prePositionNeighbour(neighbour: WebView, sign: Int) {
        val forward = (sign > 0) != isRightToLeft
        if (preRastered !== neighbour) {
            preRastered?.settings?.offscreenPreRaster = false
            neighbour.settings.offscreenPreRaster = true
            preRastered = neighbour
        }
        val js = if (forward) "readium.scrollToStart();" else "readium.scrollToEnd();"
        neighbour.evaluateJavascript(js, null)
    }

    private fun rearmFakeDrag(currentPager: ViewPager) {
        if (!fakeDragArmNeeded) return
        fakeDragArmNeeded = false
        currentPager.beginFakeDrag()
    }

    private fun closeFakeDrag() {
        val currentPager = pager?.takeIf { it.isFakeDragging } ?: return
        rearmFakeDrag(currentPager)
        runCatching {
            currentPager.fakeDragBy((currentPager.scrollX - basePagerScrollX).toFloat())
            currentPager.endFakeDrag()
        }
        preRastered?.settings?.offscreenPreRaster = false
        preRastered = null
        fakeDragArmNeeded = false
        outerOffset = hostView.width.toFloat()
        setChildTranslationX(0f)
    }

    private fun childTranslationX(): Float = (hostView as? ViewGroup)?.getChildAt(0)?.translationX ?: 0f

    private fun setChildTranslationX(value: Float) {
        (hostView as? ViewGroup)?.getChildAt(0)?.translationX = value
    }

    private fun webViewsIn(root: View): List<WebView> {
        val found = mutableListOf<WebView>()
        fun walk(view: View) {
            if (view is WebView) found += view
            if (view is ViewGroup) {
                for (index in 0 until view.childCount) walk(view.getChildAt(index))
            }
        }
        walk(root)
        return found
    }

    private fun visibleWebView(root: View): WebView? = webViewsIn(root).maxByOrNull { candidate ->
        Rect().let { rect -> if (candidate.getGlobalVisibleRect(rect)) rect.width() else 0 }
    }

    private fun neighbourWebView(root: View, current: WebView, towardRight: Boolean): WebView? {
        val currentX = current.screenX()
        val candidates = webViewsIn(root).filter { it !== current }
        return if (towardRight) {
            candidates.filter { it.screenX() > currentX }.minByOrNull { it.screenX() }
        } else {
            candidates.filter { it.screenX() < currentX }.maxByOrNull { it.screenX() }
        }
    }

    private fun pagerIn(root: View): ViewPager? {
        fun walk(view: View): ViewPager? {
            if (view is ViewPager) return view
            if (view is ViewGroup) {
                for (index in 0 until view.childCount) walk(view.getChildAt(index))?.let { return it }
            }
            return null
        }
        return walk(root)
    }

    private fun View.screenX(): Int = IntArray(2).also(::getLocationOnScreen)[0]
}
