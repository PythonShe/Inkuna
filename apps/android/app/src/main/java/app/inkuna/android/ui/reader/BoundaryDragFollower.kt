package app.inkuna.android.ui.reader

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Rect
import android.os.SystemClock
import android.view.MotionEvent
import android.view.View
import android.view.ViewConfiguration
import android.view.ViewGroup
import android.webkit.WebView
import android.widget.FrameLayout
import androidx.viewpager.widget.ViewPager
import kotlin.math.abs
import org.readium.r2.navigator.epub.EpubNavigatorFragment
import org.readium.r2.navigator.preferences.ReadingProgression
import org.readium.r2.shared.ExperimentalReadiumApi

/**
 * Shared between [BoundaryDragFollower] and [BoundaryFlingRescue]: when the
 * follower drove a boundary gesture itself, the rescue (fed the same gesture
 * through Readium's drag events) must not re-drive the turn.
 */
class BoundaryGestureSignal {
    var lastHandledUptime = 0L

    fun handledRecently(): Boolean =
        SystemClock.uptimeMillis() - lastHandledUptime < SUPPRESS_WINDOW_MS

    private companion object {
        /** Readium's drag-end arrives via the JS bridge well under this. */
        const val SUPPRESS_WINDOW_MS = 600L
    }
}

/**
 * Makes chapter-boundary swipes follow the finger, like iOS.
 *
 * Within a resource, Readium's paginated EPUB pages with the WebView's own
 * native column scrolling, so the page tracks the finger. But the toolkit's
 * `R2ViewPager` deliberately refuses touch for EPUB, so crossing a resource
 * (chapter) boundary can only ever happen programmatically on release: the
 * drag itself moves nothing, and the turn lands as a jump. (Upstream:
 * readium/kotlin-toolkit#158; their experimental Compose navigator owns the
 * drag properly and will obsolete this.)
 *
 * This layout wraps the navigator's fragment container. When a horizontal
 * drag starts on a resource edge that is clamped in the drag's direction —
 * the one case where the WebView underneath has nothing left to do — it
 * intercepts the gesture and replays it into the resource pager through
 * ViewPager's public fake-drag API, so the neighbouring chapter physically
 * slides in under the finger. `endFakeDrag` then settles with standard
 * pager physics: past ~half a page or flung it commits, otherwise it snaps
 * back. Readium's own `onPageSelected` bookkeeping runs on commit exactly
 * as for a programmatic turn.
 *
 * Everything here probes public view APIs only (the toolkit's classes are
 * `internal`) and no-ops gracefully when the hierarchy changes shape.
 */
@OptIn(ExperimentalReadiumApi::class)
class BoundaryDragFollower(
    context: Context,
    private val signal: BoundaryGestureSignal,
) : FrameLayout(context) {

    /** The live navigator; set by the host, cleared when it goes away. */
    var navigator: EpubNavigatorFragment? = null

    private val touchSlop = ViewConfiguration.get(context).scaledTouchSlop

    private var activePointerId = MotionEvent.INVALID_POINTER_ID
    private var downX = 0f
    private var downY = 0f
    private var lastX = 0f

    /** This gesture was examined and declined; stop looking at it. */
    private var rejected = false

    /** This view owns the gesture; a fake drag is in flight or pending. */
    private var dragging = false

    /**
     * The fake drag must not start inside the intercept pass: intercepting
     * makes the framework deliver ACTION_CANCEL to the child tree, and the
     * toolkit's ViewPager responds to that cancel by recycling its velocity
     * tracker — the one `beginFakeDrag` just created — so the first
     * `fakeDragBy` would crash on a null tracker. Deferred to the first
     * event this view owns, which arrives after the cancel has landed.
     */
    private var beginPending = false

    private var pager: ViewPager? = null

    /** Cumulative fake-drag offset, clamped to the revealing side only. */
    private var dragTotal = 0f

    /** -1 while revealing the layout-right neighbour, +1 the left one. */
    private var revealSign = 0

    override fun onInterceptTouchEvent(ev: MotionEvent): Boolean {
        when (ev.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                endDrag()
                rejected = false
                activePointerId = ev.getPointerId(0)
                downX = ev.x
                downY = ev.y
                lastX = ev.x
            }
            MotionEvent.ACTION_MOVE -> {
                if (!dragging && !rejected) considerIntercept(ev)
            }
            // A second finger means pinch/selection territory, not a page
            // turn; leave the gesture alone for good.
            MotionEvent.ACTION_POINTER_DOWN -> rejected = true
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> endDrag()
        }
        return dragging
    }

    @SuppressLint("ClickableViewAccessibility")
    override fun onTouchEvent(ev: MotionEvent): Boolean {
        if (!dragging) return false
        when (ev.actionMasked) {
            MotionEvent.ACTION_MOVE -> {
                val index = ev.findPointerIndex(activePointerId)
                if (index < 0) return true
                val x = ev.getX(index)
                if (beginPending) {
                    // Mid-settle the pager refuses fake drags; keep asking —
                    // the settle usually finishes under the same finger.
                    if (pager?.beginFakeDrag() == true) {
                        beginPending = false
                    } else {
                        lastX = x
                        return true
                    }
                }
                dragBy(x - lastX)
                lastX = x
            }
            MotionEvent.ACTION_POINTER_UP -> {
                if (ev.getPointerId(ev.actionIndex) == activePointerId) {
                    finishGesture(cancelled = false)
                }
            }
            MotionEvent.ACTION_UP -> finishGesture(cancelled = false)
            MotionEvent.ACTION_CANCEL -> finishGesture(cancelled = true)
        }
        return true
    }

    /**
     * The WebView underneath scrolls its own columns and may ask parents
     * not to intercept while it does. At a clamped boundary it has nothing
     * to scroll, and honouring the request would kill the one feature this
     * view exists for — so it is deliberately not propagated. Nothing above
     * this view competes for horizontal drags.
     */
    override fun requestDisallowInterceptTouchEvent(disallowIntercept: Boolean) {}

    private fun considerIntercept(ev: MotionEvent) {
        val index = ev.findPointerIndex(activePointerId)
        if (index < 0) {
            rejected = true
            return
        }
        val dx = ev.getX(index) - downX
        val dy = ev.getY(index) - downY
        // Vertical intent: this gesture is never becoming a page turn.
        if (abs(dy) > touchSlop && abs(dy) > abs(dx)) {
            rejected = true
            return
        }
        if (abs(dx) <= touchSlop || abs(dx) < 2 * abs(dy)) return

        val nav = navigator ?: run { rejected = true; return }
        // Scroll mode turns pages by overscroll; nothing to follow.
        if (nav.overflow.value.scroll) {
            rejected = true
            return
        }
        val root = nav.view ?: run { rejected = true; return }
        val webView = visibleWebView(root)
        val foundPager = pagerIn(root)
        // Column offsets grow left-to-right regardless of the reading
        // progression, so both clamp tests are geometric: the WebView must
        // have nothing left in the drag's direction, and the pager must
        // have a neighbour there.
        val ahead = if (dx < 0) 1 else -1
        val blocked = webView != null && !webView.canScrollHorizontally(ahead)
        val pagerHasNeighbour = foundPager != null && foundPager.canScrollHorizontally(ahead)
        if (!blocked || !pagerHasNeighbour) {
            rejected = true
            return
        }

        pager = foundPager
        dragging = true
        beginPending = true
        revealSign = if (dx < 0) -1 else 1
        dragTotal = 0f
        lastX = ev.getX(index)
        prePositionNeighbour(nav, root, webView)
    }

    /**
     * Feeds finger deltas to the pager, clamped to the revealing side: past
     * the gesture's origin the pager would start exposing the *other*
     * neighbour, which the WebView's own columns are responsible for.
     */
    private fun dragBy(delta: Float) {
        val pager = pager ?: return
        if (!pager.isFakeDragging) return
        val limit = width.toFloat()
        val clampedTotal = (dragTotal + delta).coerceIn(
            if (revealSign < 0) -limit else 0f,
            if (revealSign < 0) 0f else limit,
        )
        val applied = clampedTotal - dragTotal
        dragTotal = clampedTotal
        if (applied != 0f) pager.fakeDragBy(applied)
    }

    /**
     * Releases the gesture. The suppression signal is stamped only when
     * this view actually drove a turn (fake drag or direct) — stamping the
     * declined paths would silently swallow the gesture instead of leaving
     * it to BoundaryFlingRescue.
     */
    private fun finishGesture(cancelled: Boolean) {
        val pager = pager
        if (pager != null && pager.isFakeDragging) {
            // A cancel is not a release: unwind first so it cannot commit
            // a turn the reader never let go into (ViewPager's own touch
            // path snaps back on cancel the same way).
            if (cancelled) dragBy(-dragTotal)
            pager.endFakeDrag()
            signal.lastHandledUptime = SystemClock.uptimeMillis()
        } else if (!cancelled && beginPending) {
            // The pager refused the fake drag for the whole gesture (still
            // settling the previous turn) but the touch stream was already
            // stolen, so nothing else saw it: commit deliberate drags
            // through the navigator, mirroring the rescue's ⅓-page rule.
            val nav = navigator
            if (nav != null && width > 0 && abs(lastX - downX) >= width / 3f) {
                val rtl = nav.overflow.value.readingProgression == ReadingProgression.RTL
                val forward = (revealSign < 0) != rtl
                if (forward) nav.goForward(animated = true) else nav.goBackward(animated = true)
                signal.lastHandledUptime = SystemClock.uptimeMillis()
            }
        }
        endDrag()
    }

    /** Bare state reset; any stranded fake drag is closed without acting. */
    private fun endDrag() {
        pager?.takeIf { it.isFakeDragging }?.endFakeDrag()
        dragging = false
        beginPending = false
        pager = null
        activePointerId = MotionEvent.INVALID_POINTER_ID
    }

    /**
     * Scrolls the about-to-be-revealed resource to the page the reader
     * expects: first page when moving forward, last when moving backward.
     * Readium only does this in `onPageSelected` — after the commit — so a
     * previously visited neighbour would otherwise slide in showing
     * whatever page it was last left on. `readium.*` is the toolkit's own
     * JS runtime, present in every resource WebView; the snap is
     * column-aligned and RTL-aware. Readium re-runs its own positioning on
     * commit, so this can never leave stale state behind.
     */
    private fun prePositionNeighbour(
        nav: EpubNavigatorFragment,
        root: View,
        current: WebView?,
    ) {
        current ?: return
        val rtl = nav.overflow.value.readingProgression == ReadingProgression.RTL
        val forward = (revealSign < 0) != rtl
        val neighbour = neighbourWebView(root, current, towardRight = revealSign < 0) ?: return
        val js = if (forward) "readium.scrollToStart();" else "readium.scrollToEnd();"
        neighbour.evaluateJavascript(js, null)
    }

    // View-hierarchy probes. The pager keeps neighbouring resources
    // instantiated, so several WebViews coexist; the visible one covers
    // the most screen and neighbours sit beside it in layout space.

    private fun webViewsIn(root: View): List<WebView> {
        val found = mutableListOf<WebView>()
        fun walk(view: View) {
            if (view is WebView) found += view
            if (view is ViewGroup) {
                for (i in 0 until view.childCount) walk(view.getChildAt(i))
            }
        }
        walk(root)
        return found
    }

    private fun visibleWebView(root: View): WebView? =
        webViewsIn(root).maxByOrNull { webView ->
            Rect().let { rect ->
                if (webView.getGlobalVisibleRect(rect)) rect.width() else 0
            }
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
                for (i in 0 until view.childCount) {
                    walk(view.getChildAt(i))?.let { return it }
                }
            }
            return null
        }
        return walk(root)
    }

    private fun View.screenX(): Int = IntArray(2).also(::getLocationOnScreen)[0]
}
