package app.inkuna.android.ui.reader

import android.graphics.Rect
import android.os.SystemClock
import android.view.Choreographer
import android.view.View
import android.view.ViewGroup
import android.webkit.WebView
import java.lang.ref.WeakReference
import kotlin.math.abs
import kotlin.math.sign
import org.readium.r2.navigator.epub.EpubNavigatorFragment
import org.readium.r2.navigator.input.DragEvent
import org.readium.r2.navigator.input.InputListener
import org.readium.r2.shared.ExperimentalReadiumApi
import org.readium.r2.navigator.preferences.ReadingProgression

/**
 * Rescues page swipes that die at a resource (chapter) boundary.
 *
 * Within a resource, Readium's paginated EPUB pages with the WebView's own
 * native column fling, which is why intra-chapter swiping feels light. But
 * the toolkit disables its ViewPager's gesture handling for EPUB entirely;
 * crossing a resource hangs on a hand-rolled fling gate in `R2WebView`
 * that dropped ViewPager's "dragged far enough commits anyway" fallback
 * and samples its initial velocity before touch slop — so a normal flick
 * at a chapter's edge frequently snaps back, and only a forceful long drag
 * released at speed gets through.
 *
 * This listener re-runs fling detection over the same drag stream (the
 * events carry no velocity, so it timestamps them itself) and, when a
 * legitimate fling pointed at an edge went nowhere, drives the turn
 * through the navigator's own [EpubNavigatorFragment.goForward]/
 * [EpubNavigatorFragment.goBackward] — which are edge-aware and handle
 * RTL internally.
 *
 * It also commits slow-but-deliberate drags: Readium's gate is
 * velocity-only, so a drag released without speed snaps back no matter
 * how far the finger travelled. Past [DRAG_COMMIT_FRACTION] of the page
 * width the turn commits on release, matching ViewPager's
 * "dragged far enough commits anyway" rule that the toolkit dropped.
 */
@OptIn(ExperimentalReadiumApi::class)
class BoundaryFlingRescue(
    private val navigator: EpubNavigatorFragment,
    density: Float,
    /** True while [BoundaryDragFollower] drove this same gesture itself. */
    private val isSuppressed: () -> Boolean = { false },
) : InputListener {

    private data class Sample(val time: Long, val x: Float)

    /** Recent drag samples, pruned to the last [VELOCITY_WINDOW_MS]. */
    private val samples = ArrayDeque<Sample>()
    private val minDistancePx = MIN_DISTANCE_DP * density
    private val minVelocityPx = MIN_VELOCITY_DP_S * density

    /** The armed verification, so a fresh drag can supersede it. */
    private var pendingVerify: Choreographer.FrameCallback? = null

    // The resource WebView as the drag began: whether each direction was
    // already clamped (a true edge), and where the view sat on screen so
    // a turn Readium *did* perform can be told apart from a dead swipe.
    private var startWebView: WeakReference<WebView>? = null
    private var startScreenX = 0
    private var startPageWidth = 0
    private var edgeForward = false
    private var edgeBackward = false

    override fun onDrag(event: DragEvent): Boolean {
        // Scroll mode turns pages by overscroll, not by fling; vertical
        // scrolled text drags vertically. Neither needs rescuing.
        if (navigator.overflow.value.scroll) return false
        val now = SystemClock.uptimeMillis()
        when (event.type) {
            DragEvent.Type.Start -> {
                cancelVerify()
                samples.clear()
                samples += Sample(now, event.offset.x)
                val webView = visibleWebView()
                startWebView = webView?.let(::WeakReference)
                startScreenX = webView?.screenX() ?: 0
                startPageWidth = webView?.width ?: 0
                // Column offsets grow left-to-right regardless of the
                // reading progression, so the clamp test is geometric.
                edgeForward = webView != null && !webView.canScrollHorizontally(1)
                edgeBackward = webView != null && !webView.canScrollHorizontally(-1)
            }
            DragEvent.Type.Move -> {
                samples += Sample(now, event.offset.x)
                while (samples.isNotEmpty() && now - samples.first().time > VELOCITY_WINDOW_MS) {
                    samples.removeFirst()
                }
            }
            DragEvent.Type.End -> maybeRescue(now, event)
        }
        // True would propagate to preventDefault in the toolkit's gesture
        // script and kill native column paging; this listener only watches.
        return false
    }

    private fun maybeRescue(now: Long, event: DragEvent) {
        // The follower already replayed this gesture into the pager with
        // full visual feedback; re-driving it would turn twice (on commit)
        // or override the reader's cancel (on snap-back).
        if (isSuppressed()) return
        val dx = event.offset.x
        val dy = event.offset.y
        if (abs(dx) < minDistancePx || abs(dx) < 2 * abs(dy)) return
        // Anywhere but a clamped edge the native fling already turned the
        // page; acting there would double the turn.
        val blocked = if (dx < 0) edgeForward else edgeBackward
        if (!blocked) return

        val first = samples.firstOrNull() ?: return
        val elapsed = (now - first.time).coerceAtLeast(1L)
        val velocityX = (dx - first.x) / elapsed * 1000f
        // Two ways to commit: a live fling in the drag's direction, or a
        // drag carried far enough that speed no longer matters. A live
        // fling *against* the drag is the reader changing their mind.
        val flung = abs(velocityX) >= minVelocityPx
        val committed = when {
            flung -> sign(velocityX) == sign(dx)
            else -> startPageWidth > 0 && abs(dx) >= startPageWidth * DRAG_COMMIT_FRACTION
        }
        if (!committed) return

        val rtl = navigator.overflow.value.readingProgression == ReadingProgression.RTL
        val forward = (dx < 0) != rtl

        // Readium's own gate occasionally passes (the force-drag case) and
        // moves its pager within a frame of the release. Watch at frame
        // rate instead of sleeping a blind wall-clock beat: any movement
        // of the dragged WebView means Readium acted — stand down; a view
        // still holding its exact screen position after a few quiet
        // frames proves the swipe died, and the turn fires right then.
        // (The current locator is debounced and lags animations; the view
        // check is race-free.) The old 250 ms sleep made every rescued
        // turn eat a quarter second before anything moved.
        val webViewRef = startWebView
        val screenX = startScreenX
        cancelVerify()
        val armedAt = now
        var quietFrames = 0
        val choreographer = Choreographer.getInstance()
        val callback = object : Choreographer.FrameCallback {
            override fun doFrame(frameTimeNanos: Long) {
                if (pendingVerify !== this) return
                val webView = webViewRef?.get()
                if (webView == null || !webView.isAttachedToWindow ||
                    webView.screenX() != screenX
                ) {
                    pendingVerify = null
                    return
                }
                quietFrames += 1
                val quietMs = SystemClock.uptimeMillis() - armedAt
                if (quietFrames >= QUIET_FRAMES && quietMs >= QUIET_MS) {
                    pendingVerify = null
                    if (forward) {
                        navigator.goForward(animated = true)
                    } else {
                        navigator.goBackward(animated = true)
                    }
                } else {
                    choreographer.postFrameCallback(this)
                }
            }
        }
        pendingVerify = callback
        choreographer.postFrameCallback(callback)
    }

    /**
     * Drops any armed verification. The reader calls this when it removes
     * the listener: the `pendingVerify !== this` self-guard already makes
     * a superseded callback inert, but an armed one would otherwise
     * outlive the listener's removal and could still drive a turn during
     * teardown — and removing the callback also saves the frame dispatch.
     */
    fun cancel() {
        pendingVerify?.let(Choreographer.getInstance()::removeFrameCallback)
        pendingVerify = null
    }

    private fun cancelVerify() = cancel()

    /**
     * The resource WebView currently on screen. The pager keeps neighbours
     * instantiated, so pick the one covering the most of the screen.
     */
    private fun visibleWebView(): WebView? {
        val root = navigator.view ?: return null
        val found = mutableListOf<WebView>()
        fun walk(view: View) {
            if (view is WebView) found += view
            if (view is ViewGroup) {
                for (i in 0 until view.childCount) walk(view.getChildAt(i))
            }
        }
        walk(root)
        return found.maxByOrNull { webView ->
            Rect().let { rect ->
                if (webView.getGlobalVisibleRect(rect)) rect.width() else 0
            }
        }
    }

    private fun View.screenX(): Int = IntArray(2).also(::getLocationOnScreen)[0]

    private companion object {
        const val VELOCITY_WINDOW_MS = 120L

        /**
         * How long the released WebView must hold perfectly still before
         * the rescue concludes Readium is not acting. Readium's own gate
         * moves its pager within a frame of the drag-end it accepts, so
         * a handful of quiet frames — with a wall-clock floor for very
         * fast displays — is proof; ~48 ms at 60 Hz, ~33 ms at 120 Hz.
         */
        const val QUIET_FRAMES = 3
        const val QUIET_MS = 32L
        const val MIN_DISTANCE_DP = 24f
        /** Deliberately below Readium's 400 dp/s gate — a light flick. */
        const val MIN_VELOCITY_DP_S = 300f

        /**
         * A slow drag commits past this fraction of the page width. The
         * boundary gives no visual feedback while dragging (the next
         * resource lives in another WebView), so this is deliberately
         * shy of ViewPager's half-page rule.
         */
        const val DRAG_COMMIT_FRACTION = 1f / 3f
    }
}
