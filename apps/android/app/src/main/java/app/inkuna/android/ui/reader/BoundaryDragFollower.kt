package app.inkuna.android.ui.reader

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Rect
import android.os.Build
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
 *
 * In practice an intercepted gesture never even reaches the rescue:
 * Readium's gesture script listens for `touchend` but not `touchcancel`,
 * and interception delivers exactly a cancel — so the drag-End the rescue
 * keys on dies with the intercept. The window is belt and braces for the
 * follower's navigator-fallback path, not a load-bearing gate; do not
 * tune gesture feel around it.
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

/** Residual scroll under this still reads as a clamped resource edge. */
private const val CLAMP_SLOP_DP = 4f

/** A release at this speed commits even without the ⅓-page travel —
 *  matching [BoundaryFlingRescue]'s light-flick gate. */
private const val MIN_FLING_VELOCITY_DP_S = 300f

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

    /** How much residual scroll still counts as a clamped resource edge. */
    private val clampSlopPx =
        (CLAMP_SLOP_DP * context.resources.displayMetrics.density).toInt().coerceAtLeast(1)

    private val minFlingVelocityPx =
        MIN_FLING_VELOCITY_DP_S * context.resources.displayMetrics.density

    private var activePointerId = MotionEvent.INVALID_POINTER_ID
    private var downX = 0f
    private var downY = 0f
    private var lastX = 0f
    private var lastMoveUptime = 0L

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

    /** The finger x the fake drag maps from, and the pager offset under it. */
    private var fingerOrigin = 0f
    private var startScrollX = 0

    /** -1 while revealing the layout-right neighbour, +1 the left one. */
    private var revealSign = 0

    /**
     * The neighbour whose off-screen pre-raster this gesture switched on.
     * `offscreenPreRaster` significantly increases a WebView's memory use
     * and is documented for exactly this reveal window; it is switched
     * back off when the gesture releases — by then the neighbour is
     * either on screen (rasterized as any visible view) or staying put.
     */
    private var preRastered: WebView? = null

    override fun onInterceptTouchEvent(ev: MotionEvent): Boolean {
        when (ev.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                endDrag()
                rejected = false
                activePointerId = ev.getPointerId(0)
                downX = ev.x
                downY = ev.y
                lastX = ev.x
                lastMoveUptime = ev.eventTime
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
                    val pager = pager
                    // ViewPager 1.1.0 refuses a fake drag only while a
                    // real touch drag is in flight, which the EPUB
                    // pager's rejected ACTION_DOWN makes impossible — but
                    // a future toolkit must not leave the reader holding
                    // a stolen, inert touch stream.
                    if (pager == null || !pager.beginFakeDrag()) {
                        rejected = true
                        endDrag()
                        return true
                    }
                    beginPending = false
                    startScrollX = pager.scrollX
                }
                // Adaptive-refresh displays pick their rate partly from
                // content velocity, and the hint resets every frame —
                // feed the finger's instantaneous speed while it drives
                // the pager.
                if (Build.VERSION.SDK_INT >= 35) {
                    val dt = ev.eventTime - lastMoveUptime
                    if (dt > 0) {
                        pager?.frameContentVelocity = abs(x - lastX) / dt * 1000f
                    }
                }
                lastMoveUptime = ev.eventTime
                // One correction per MOVE, ignoring the batched history:
                // fakeDragBy stamps its synthetic tracker event with
                // uptimeMillis() at call time, so replaying historical
                // samples would hand the velocity tracker N points
                // sharing one timestamp — no truer release speed, just
                // N× the scroll/listener work per frame.
                dragTo(x)
                lastX = x
            }
            MotionEvent.ACTION_POINTER_UP -> {
                if (ev.getPointerId(ev.actionIndex) == activePointerId) {
                    finishGesture(cancelled = false, ev)
                }
            }
            MotionEvent.ACTION_UP -> finishGesture(cancelled = false, ev)
            MotionEvent.ACTION_CANCEL -> finishGesture(cancelled = true, ev)
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

        // Crossing the horizontal slop only now, after a long-press has
        // had time to fire, is the signature of a text-selection handle
        // under the finger. Readium's selection state is not readable
        // synchronously, so the timeout is the guard: stealing that drag
        // would turn the chapter out from under a selection.
        if (ev.eventTime - ev.downTime >= ViewConfiguration.getLongPressTimeout()) {
            rejected = true
            return
        }

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
        // have nothing meaningful left in the drag's direction, and the
        // pager must have a neighbour there. "Nothing meaningful" carries
        // a small tolerance — an interrupted column settle routinely
        // leaves a few pixels of residue, and a strict test would stand
        // the follower down exactly when the reader swipes again at a
        // chapter's edge, handing the gesture to the rescue as a jump.
        val ahead = if (dx < 0) 1 else -1
        val blocked = webView != null && webView.clampedWithin(ahead, clampSlopPx)
        val pagerHasNeighbour = foundPager != null && foundPager.canScrollHorizontally(ahead)
        if (webView == null || !blocked || !pagerHasNeighbour) {
            rejected = true
            return
        }

        // The neighbour must exist and hold real content: sliding a blank
        // white page in under the finger is worse than declining and
        // letting the rescue turn programmatically a few frames later.
        val neighbour = neighbourWebView(root, webView, towardRight = dx < 0)
        if (neighbour == null || neighbour.progress < 100 || neighbour.contentHeight == 0) {
            rejected = true
            return
        }

        pager = foundPager
        dragging = true
        beginPending = true
        revealSign = if (dx < 0) -1 else 1
        fingerOrigin = ev.getX(index)
        lastX = fingerOrigin
        prePositionNeighbour(nav, neighbour)
    }

    /**
     * Whether the WebView cannot move more than [slop] px further toward
     * [direction]. [View.canScrollHorizontally] is a strict boolean and
     * the offsets under it are protected, so the remaining room is
     * measured with a probe: nudge by [slop] and ask the strict test
     * again from there. The nudged offset itself proves nothing —
     * `View.scrollTo` writes `mScrollX` unclamped and Chromium corrects
     * only asynchronously, so `scrollX` always reports the full nudge —
     * but `canScrollHorizontally` compares the nudged offset against the
     * renderer-supplied range, so "no room left from here" means less
     * than [slop] was available. Both writes land inside one event
     * dispatch — nothing is ever drawn displaced.
     */
    private fun WebView.clampedWithin(direction: Int, slop: Int): Boolean {
        if (!canScrollHorizontally(direction)) return true
        val before = scrollX
        scrollBy(direction * slop, 0)
        val roomLeft = canScrollHorizontally(direction)
        scrollTo(before, scrollY)
        return !roomLeft
    }

    /**
     * Maps the finger to an *absolute* pager offset and corrects toward
     * it, rather than accumulating deltas: `beginFakeDrag` does not abort
     * a Scroller settle already in flight (and the EPUB pager rejects the
     * ACTION_DOWN that would normally catch one), so during a swipe train
     * the old settle keeps writing `scrollX` between events — absolute
     * targeting re-corrects that drift on every sample instead of
     * fighting it blind.
     *
     * The total is clamped to the revealing side: past the gesture's
     * origin the pager would start exposing the *other* neighbour, which
     * the WebView's own columns are responsible for. A finger that keeps
     * going that way finds a dead zone — the intercept already cancelled
     * the WebView's touch stream and Android cannot un-cancel it; the
     * columns respond again from the next touch.
     */
    private fun dragTo(x: Float) {
        val pager = pager ?: return
        if (!pager.isFakeDragging) return
        val limit = width.toFloat()
        val total = (x - fingerOrigin).coerceIn(
            if (revealSign < 0) -limit else 0f,
            if (revealSign < 0) 0f else limit,
        )
        val delta = pager.scrollX - (startScrollX - total)
        if (delta != 0f) pager.fakeDragBy(delta)
    }

    /**
     * Releases the gesture. The suppression signal is stamped only when
     * this view actually drove a turn (fake drag or direct) — stamping the
     * declined paths would silently swallow the gesture instead of leaving
     * it to BoundaryFlingRescue.
     */
    private fun finishGesture(cancelled: Boolean, ev: MotionEvent?) {
        val pager = pager
        if (pager != null && pager.isFakeDragging) {
            // A cancel is not a release: unwind first so it cannot commit
            // a turn the reader never let go into (ViewPager's own touch
            // path snaps back on cancel the same way).
            if (cancelled) dragTo(fingerOrigin)
            // endFakeDrag reads its velocity tracker with the pager's
            // INVALID_POINTER (-1) id — which happens to also be
            // VelocityTracker's "active pointer" sentinel, so the fling
            // velocity of the per-MOVE finger samples IS honoured. A
            // lucky collision of two unrelated -1 constants; do not
            // "fix" either.
            pager.endFakeDrag()
            pumpSettleVelocity(pager)
            signal.lastHandledUptime = SystemClock.uptimeMillis()
        } else if (!cancelled && beginPending) {
            // The touch stream was stolen but the fake drag never engaged
            // — an UP hard on the heels of the intercept, so no MOVE ever
            // arrived in between. Nothing else saw this gesture (the
            // intercept's cancel killed Readium's JS drag stream too), so
            // commit what the finger clearly asked for: a flick released
            // at speed, or a drag past the shells' shared ⅓-page rule.
            val nav = navigator
            val dx = lastX - downX
            if (nav != null && width > 0 && ev != null && abs(dx) > touchSlop) {
                val elapsed = (ev.eventTime - ev.downTime).coerceAtLeast(1)
                val velocity = abs(dx) / elapsed * 1000f
                if (abs(dx) >= width / 3f || velocity >= minFlingVelocityPx) {
                    val rtl = nav.overflow.value.readingProgression == ReadingProgression.RTL
                    val forward = (revealSign < 0) != rtl
                    if (forward) nav.goForward(animated = true) else nav.goBackward(animated = true)
                    signal.lastHandledUptime = SystemClock.uptimeMillis()
                }
            }
        }
        endDrag()
    }

    /**
     * Keeps the frame-rate content-velocity hint alive through the settle
     * `endFakeDrag` kicked off: the hint resets on every redraw, and
     * nothing in ViewPager feeds it, so without this the display can
     * drop back to its idle rate mid-slide. Stops once the pager holds
     * still for a couple of frames or the pager's own settle ceiling
     * (600 ms) has passed.
     */
    private fun pumpSettleVelocity(pager: ViewPager) {
        if (Build.VERSION.SDK_INT < 35) return
        val deadline = SystemClock.uptimeMillis() + 600
        pager.postOnAnimation(object : Runnable {
            var lastScrollX = pager.scrollX
            var lastTime = SystemClock.uptimeMillis()
            var stillFrames = 0

            override fun run() {
                val now = SystemClock.uptimeMillis()
                val dx = pager.scrollX - lastScrollX
                val dt = now - lastTime
                if (dx == 0) {
                    stillFrames += 1
                } else {
                    stillFrames = 0
                    if (dt > 0) pager.frameContentVelocity = abs(dx).toFloat() / dt * 1000f
                }
                lastScrollX = pager.scrollX
                lastTime = now
                if (stillFrames < 2 && now < deadline && pager.isAttachedToWindow) {
                    pager.postOnAnimation(this)
                }
            }
        })
    }

    /** Bare state reset; any stranded fake drag is closed without acting. */
    private fun endDrag() {
        pager?.takeIf { it.isFakeDragging }?.endFakeDrag()
        preRastered?.settings?.offscreenPreRaster = false
        preRastered = null
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
    private fun prePositionNeighbour(nav: EpubNavigatorFragment, neighbour: WebView) {
        val rtl = nav.overflow.value.readingProgression == ReadingProgression.RTL
        val forward = (revealSign < 0) != rtl
        // Rasterize the incoming resource before it slides on screen —
        // bounded to the one neighbour a drag can reveal, and switched
        // back off in endDrag.
        neighbour.settings.offscreenPreRaster = true
        preRastered = neighbour
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
