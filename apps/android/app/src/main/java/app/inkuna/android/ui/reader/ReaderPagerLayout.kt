package app.inkuna.android.ui.reader

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Rect
import android.os.Build
import android.view.MotionEvent
import android.view.VelocityTracker
import android.view.View
import android.view.ViewConfiguration
import android.view.ViewGroup
import android.webkit.WebView
import android.widget.FrameLayout
import androidx.viewpager.widget.ViewPager
import kotlin.math.abs
import kotlin.math.roundToInt
import org.readium.r2.navigator.epub.EpubNavigatorFragment
import org.readium.r2.navigator.preferences.ReadingProgression
import org.readium.r2.shared.ExperimentalReadiumApi

/**
 * The reader's own pager: every horizontal page turn — within a resource
 * and across resource (chapter) boundaries — is driven here, on our
 * gesture pipeline and our physics, instead of Readium's. This layout
 * wraps the navigator's fragment container and claims every horizontal
 * drag at plain touch slop, before Chromium's native pan can engage;
 * the child tree receives a cancel, which also kills Readium's JS drag
 * stream — so the toolkit's hand-rolled fling gate (which drops normal
 * flicks at chapter edges) and its column scrolling are both starved
 * without touching a single Readium internal.
 *
 * The model is one continuous strip. A drag accumulates into a raw strip
 * coordinate; the resource's columns consume it first (driven with
 * `View.scrollTo` on the WebView — the exact mechanism Readium's own
 * settle Scroller uses), and whatever overflows a clamped edge spills
 * into the resource pager through ViewPager's public fake-drag API, so
 * a single gesture flows from the last page of a chapter straight into
 * dragging the next chapter in, both live under the finger. Reversing
 * unwinds in the same order. Purely geometric: WebView and pager scroll
 * offsets both grow left-to-right regardless of reading progression, so
 * RTL only matters when mapping "forward" for taps and keys.
 *
 * Release physics and thresholds are shared with the iOS shell: commits
 * take a third of a page or a flick (an opposing flick always cancels),
 * and every settle is a critically damped [SettleSpring] fed the
 * finger's release velocity. A boundary settle lands on the exact page
 * offset at ~zero velocity *before* `endFakeDrag`, so ViewPager's own
 * target computation deterministically commits that page and Readium's
 * `onPageSelected` bookkeeping runs exactly as for a programmatic turn.
 * Every flight is interruptible: a touch-down freezes it in place, a
 * drag from there picks it up under the finger, and a bare tap lets it
 * finish — the touch stream itself stays with the page (so Readium's
 * JS taps still fire, and a tap on a settling page chains the turn
 * through the spring's retarget instead of being swallowed).
 *
 * Everything here probes public view APIs only (the toolkit's classes
 * are `internal`) and no-ops gracefully when the hierarchy changes
 * shape. Known trade-off of full takeover: horizontally scrollable
 * elements *inside* a page (wide tables) lose native panning.
 */
@OptIn(ExperimentalReadiumApi::class)
class ReaderPagerLayout(context: Context) : FrameLayout(context) {

    /** The live navigator; set by the host, cleared when it goes away. */
    var navigator: EpubNavigatorFragment? = null

    /** Fired the moment a drag is claimed, so the chrome can clear
     *  before the motion, not after the locator lands. */
    var onTurnGesture: (() -> Unit)? = null

    private val touchSlop = ViewConfiguration.get(context).scaledTouchSlop
    private val minFlingVelocityPx =
        MIN_FLING_VELOCITY_DP_S * context.resources.displayMetrics.density

    // MARK: Gesture state

    private var activePointerId = MotionEvent.INVALID_POINTER_ID
    private var downX = 0f
    private var downY = 0f
    private var lastX = 0f
    private var lastMoveUptime = 0L

    /** This gesture was examined and declined; stop looking at it. */
    private var rejected = false

    /** This view owns the gesture. */
    private var dragging = false

    /** A settle frozen by a touch-down, waiting to be picked up by a
     *  drag or resumed by the touch ending. [Settle.NONE] means none. */
    private var frozen = Settle.NONE
    private var frozenTarget = 0f
    private var frozenVelocity = 0f

    private var velocityTracker: VelocityTracker? = null

    // MARK: Strip state (one interaction)

    private var webView: WebView? = null
    private var pager: ViewPager? = null

    /**
     * The column pitch in device px. Seeded from the view width and
     * refined by the page's own geometry over the JS bridge: Readium's
     * CSS columns are *not* always exactly a view-width apart (content
     * width need not divide evenly), and every page target must sit on
     * the true grid or the error accumulates a visible sliver per turn.
     */
    private var innerPitch = 0f
    private var pitchSource: WebView? = null

    /** The finger x that maps to [baseStrip]; strip = base + (anchor − x). */
    private var anchorX = 0f
    private var baseStrip = 0f

    /**
     * The columns' scroll ceiling, in device px. Seeded by an edge probe
     * at claim (exact at the boundaries, where it matters most) and
     * refined by the page's own geometry over the JS bridge — the
     * `computeHorizontalScrollRange` the View API keeps protected.
     * [Int.MAX_VALUE] means "not yet known": the strip then simply
     * cannot overflow forward, which a frame later it can.
     */
    private var innerMax = Int.MAX_VALUE
    private var innerMaxGeneration = 0

    /** The column page the gesture began on, for the commit rule. */
    private var startPage = 0

    /** The pager offset the fake drag corrects against. */
    private var basePagerScrollX = 0

    /** Displayed pager displacement (+ reveals the layout-right, i.e.
     *  higher-scrollX, neighbour). Nonzero means a boundary is open. */
    private var boundaryPx = 0f

    /** Raw overflow currently rubber-banded (no neighbour to reveal). */
    private var rubberRaw = 0f

    /** Per-gesture neighbour verdicts: 0 unknown, 1 ready, -1 declined. */
    private var neighbourReadyPlus = 0
    private var neighbourReadyMinus = 0

    /**
     * The neighbour whose off-screen pre-raster this gesture switched
     * on. `offscreenPreRaster` significantly increases a WebView's
     * memory use and is documented for exactly this reveal window; it
     * is switched back off when the interaction fully settles.
     */
    private var preRastered: WebView? = null

    // MARK: Settle state

    private val spring = SettleSpring()
    private enum class Settle { NONE, INNER, PAGER_COMMIT_PLUS, PAGER_COMMIT_MINUS, PAGER_RETURN, RUBBER }
    private var settle = Settle.NONE

    // MARK: Touch pipeline

    override fun onInterceptTouchEvent(ev: MotionEvent): Boolean {
        when (ev.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                rejected = false
                activePointerId = ev.getPointerId(0)
                downX = ev.x
                downY = ev.y
                lastX = ev.x
                lastMoveUptime = ev.eventTime
                velocityTracker?.recycle()
                velocityTracker = VelocityTracker.obtain().also { it.addMovement(ev) }
                if (spring.isRunning) {
                    // Freeze the flight in place, but let the touch
                    // stream through: a drag from here picks the page up
                    // (no slop to Chromium's pan — ours claims first),
                    // while a bare tap resumes the settle on the way out
                    // and still reaches Readium's JS tap recognizer —
                    // which is how tap-chained turns hit the retarget
                    // path instead of being swallowed.
                    freezeSettle()
                }
            }
            MotionEvent.ACTION_MOVE -> {
                if (!dragging && !rejected) {
                    velocityTracker?.addMovement(ev)
                    if (frozen != Settle.NONE) claimFrozen(ev) else considerClaim(ev)
                }
            }
            // A second finger means pinch/selection territory, not a page
            // turn; leave the gesture alone for good.
            MotionEvent.ACTION_POINTER_DOWN -> if (!dragging) rejected = true
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> if (!dragging) {
                // Resume first: reset()'s stranded-drag close must see
                // the spring running again and stand down.
                resumeFrozen()
                reset()
            }
        }
        return dragging
    }

    @SuppressLint("ClickableViewAccessibility")
    override fun onTouchEvent(ev: MotionEvent): Boolean {
        if (!dragging) return false
        velocityTracker?.addMovement(ev)
        when (ev.actionMasked) {
            MotionEvent.ACTION_MOVE -> {
                val index = ev.findPointerIndex(activePointerId)
                if (index < 0) return true
                val x = ev.getX(index)
                feedFingerVelocity(x, ev.eventTime)
                // One correction per MOVE, ignoring the batched history:
                // replaying historical samples multiplies the scroll and
                // listener work per frame without a truer position.
                applyStrip(baseStrip + (anchorX - x))
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
     * not to intercept while it does. Its native pan never engages under
     * this regime (the claim lands at plain touch slop, below Chromium's
     * own), and honouring the request would hand the gesture back — so it
     * is deliberately not propagated. Nothing above this view competes
     * for horizontal drags.
     */
    @Suppress("EmptyFunctionBlock")
    override fun requestDisallowInterceptTouchEvent(disallowIntercept: Boolean) {}

    override fun onDetachedFromWindow() {
        cancelInteraction()
        super.onDetachedFromWindow()
    }

    // MARK: Claiming

    private fun considerClaim(ev: MotionEvent) {
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
        if (abs(dx) <= touchSlop || abs(dx) < abs(dy)) return

        // While a selection is up, horizontal drags belong to its
        // handles. `startedSince` backs the flag up: a handle grabbed in
        // the beat before the ActionMode's start callback ran still
        // reads as selection, while a thumb merely resting on the page
        // before a swipe (which trips no selection) claims normally.
        if (SelectionModeTracker.active || SelectionModeTracker.startedSince(ev.downTime)) {
            rejected = true
            return
        }

        val nav = navigator ?: run { rejected = true; return }
        // Scroll mode has no pages to turn; stand down entirely.
        if (nav.overflow.value.scroll) {
            rejected = true
            return
        }
        val root = nav.view ?: run { rejected = true; return }
        val visible = visibleWebView(root) ?: run { rejected = true; return }
        val foundPager = pagerIn(root)

        webView = visible
        pager = foundPager
        if (visible.width <= 0) {
            rejected = true
            return
        }
        baseStrip = visible.scrollX.toFloat()
        anchorX = ev.getX(index)
        seedInnerMax(visible)
        startPage = (visible.scrollX / innerPitch).roundToInt()
        basePagerScrollX = foundPager?.scrollX ?: 0
        boundaryPx = 0f
        rubberRaw = 0f
        neighbourReadyPlus = 0
        neighbourReadyMinus = 0
        dragging = true
        onTurnGesture?.invoke()
    }

    /** A DOWN landing on a running settle: stop the flight where it is
     *  and remember it. The strip baselines from the settling gesture
     *  stay live, so a pickup or resume continues seamlessly. */
    private fun freezeSettle() {
        frozen = settle
        frozenTarget = spring.currentTarget
        frozenVelocity = spring.currentVelocity
        spring.cancel()
        settle = Settle.NONE
    }

    /** The frozen flight's touch moved past slop: the finger picks the
     *  page up, adopting the live positions as its baselines. The usual
     *  claim checks don't apply — the motion was already ours. */
    private fun claimFrozen(ev: MotionEvent) {
        val index = ev.findPointerIndex(activePointerId)
        if (index < 0) return
        if (abs(ev.getX(index) - downX) <= touchSlop) return
        val kind = frozen
        val visible = webView ?: run {
            frozen = Settle.NONE
            rejected = true
            return
        }
        val currentPager = pager
        when (kind) {
            Settle.PAGER_COMMIT_PLUS, Settle.PAGER_COMMIT_MINUS, Settle.PAGER_RETURN -> {
                if (currentPager == null || !currentPager.isFakeDragging) {
                    frozen = Settle.NONE
                    rejected = true
                    return
                }
                boundaryPx = (currentPager.scrollX - basePagerScrollX).toFloat()
                baseStrip = visible.scrollX + boundaryPx
            }
            Settle.INNER -> {
                baseStrip = visible.scrollX.toFloat()
                startPage = (visible.scrollX / innerPitch).roundToInt()
            }
            Settle.RUBBER -> {
                // Adopt the displayed (damped) travel as the raw strip;
                // resistance restarts from here — imperceptible, and
                // always convergent.
                baseStrip = visible.scrollX - childTranslationX()
            }
            Settle.NONE -> return
        }
        frozen = Settle.NONE
        anchorX = ev.getX(index)
        dragging = true
        onTurnGesture?.invoke()
    }

    /** The frozen flight's touch ended without a pickup (a tap): let the
     *  settle finish from where it froze, momentum intact. */
    private fun resumeFrozen() {
        val kind = frozen
        frozen = Settle.NONE
        when (kind) {
            Settle.INNER -> {
                val visible = webView ?: return
                startInnerSpring(visible.scrollX.toFloat(), frozenVelocity, frozenTarget)
            }
            Settle.PAGER_COMMIT_PLUS, Settle.PAGER_COMMIT_MINUS, Settle.PAGER_RETURN ->
                settlePager(frozenTarget.roundToInt(), frozenVelocity, kind)
            Settle.RUBBER -> settleRubber()
            Settle.NONE -> {}
        }
    }

    /**
     * Seeds [innerMax]. The edge probe is exact whenever the columns are
     * already clamped forward — the boundary case. Elsewhere the page's
     * own geometry answers over the JS bridge within a frame or two;
     * `readium` keeps `document.scrollingElement` as the column scroller
     * and view scroll offsets are CSS px × devicePixelRatio.
     */
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
                "return m+'|'+c;})()"
        ) { result ->
            if (generation != innerMaxGeneration || target !== webView) return@evaluateJavascript
            val parts = result?.trim('"', ' ')?.split('|') ?: return@evaluateJavascript
            val measured = parts.getOrNull(0)?.toIntOrNull() ?: return@evaluateJavascript
            val columns = parts.getOrNull(1)?.toIntOrNull() ?: return@evaluateJavascript
            // The probe's "already clamped here" verdict outranks a
            // measurement quantized across the px-ratio round-trip.
            if (innerMax == Int.MAX_VALUE) innerMax = measured
            // The true column grid: the ceiling spans columns−1 pitches.
            // Every settle target derives from this one source of truth,
            // so pages land exactly on the grid — including the last
            // one, where the edge probe needs `canScrollHorizontally`
            // to genuinely clamp.
            if (columns > 1 && innerMax in 1 until Int.MAX_VALUE) {
                innerPitch = innerMax.toFloat() / (columns - 1)
            }
        }
    }

    // MARK: The strip

    /**
     * Maps the raw strip coordinate onto the two scroll surfaces:
     * columns first, overflow to the pager — clamped to one resource
     * per gesture, rubber-banded where there is no neighbour to reveal.
     */
    private fun applyStrip(strip: Float) {
        val visible = webView ?: return
        val innerCeiling = if (innerMax == Int.MAX_VALUE) Float.MAX_VALUE else innerMax.toFloat()
        val innerTarget = strip.coerceIn(0f, innerCeiling)
        val overflow = strip - innerTarget

        val innerRounded = innerTarget.roundToInt()
        if (innerRounded != visible.scrollX) {
            visible.scrollTo(innerRounded, visible.scrollY)
        }

        val sign = if (overflow > 0f) 1 else -1
        if (overflow != 0f && neighbourReady(sign)) {
            setChildTranslationX(0f)
            rubberRaw = 0f
            drivePager(overflow.coerceIn(-width.toFloat(), width.toFloat()))
        } else {
            drivePager(0f)
            rubberRaw = overflow
            setChildTranslationX(-rubberBand(overflow))
        }
    }

    /** Whether the pager can reveal the neighbour on [sign]'s side, with
     *  the per-gesture checks (existence, loaded content, fake-drag
     *  engagement, pre-positioning) run once per side. */
    private fun neighbourReady(sign: Int): Boolean {
        val verdict = if (sign > 0) neighbourReadyPlus else neighbourReadyMinus
        if (verdict != 0) return verdict > 0
        val ready = prepareNeighbour(sign)
        if (sign > 0) neighbourReadyPlus = if (ready) 1 else -1
        else neighbourReadyMinus = if (ready) 1 else -1
        return ready
    }

    private fun prepareNeighbour(sign: Int): Boolean {
        val nav = navigator ?: return false
        val root = nav.view ?: return false
        val visible = webView ?: return false
        val currentPager = pager ?: return false
        if (!currentPager.canScrollHorizontally(sign)) return false
        // The neighbour must exist and hold real content: sliding a
        // blank white page in under the finger is worse than a rubber
        // band and an honest snap on release.
        val neighbour = neighbourWebView(root, visible, towardRight = sign > 0)
        if (neighbour == null || neighbour.progress < 100 || neighbour.contentHeight == 0) {
            return false
        }
        if (!currentPager.isFakeDragging) {
            // ViewPager 1.1.0 refuses a fake drag only while a real touch
            // drag is in flight, which the EPUB pager's rejected
            // ACTION_DOWN makes impossible — but a future toolkit must
            // not leave the reader holding a stolen, inert touch stream.
            // Beginning here — inside an owned MOVE, never the intercept
            // pass — also dodges the recycled-velocity-tracker crash the
            // intercept's child-cancel used to cause.
            if (!currentPager.beginFakeDrag()) return false
            basePagerScrollX = currentPager.scrollX
        }
        prePositionNeighbour(nav, neighbour, sign)
        return true
    }

    /**
     * Corrects the pager toward an absolute offset rather than
     * accumulating deltas: nothing else writes `scrollX` under this
     * regime, but absolute targeting keeps every sample self-healing
     * against a stray relayout.
     */
    private fun drivePager(displacement: Float) {
        val currentPager = pager ?: return
        if (!currentPager.isFakeDragging) {
            boundaryPx = 0f
            return
        }
        boundaryPx = displacement
        val target = basePagerScrollX + displacement
        val delta = currentPager.scrollX - target
        if (delta != 0f) currentPager.fakeDragBy(delta)
    }

    /**
     * Scrolls the about-to-be-revealed resource to the page the reader
     * expects: first page when moving forward in reading order, last
     * when moving backward. Readium only does this in `onPageSelected` —
     * after the commit — so a previously visited neighbour would
     * otherwise slide in showing whatever page it was last left on.
     * `readium.*` is the toolkit's own JS runtime, present in every
     * resource WebView; the snap is column-aligned and RTL-aware, and
     * Readium re-runs its own positioning on commit, so this can never
     * leave stale state behind.
     */
    private fun prePositionNeighbour(nav: EpubNavigatorFragment, neighbour: WebView, sign: Int) {
        val rtl = nav.overflow.value.readingProgression == ReadingProgression.RTL
        val forward = (sign > 0) != rtl
        // Rasterize the incoming resource before it slides on screen —
        // bounded to the one neighbour a drag can reveal, and switched
        // back off when the interaction settles.
        if (preRastered !== neighbour) {
            preRastered?.settings?.offscreenPreRaster = false
            neighbour.settings.offscreenPreRaster = true
            preRastered = neighbour
        }
        val js = if (forward) "readium.scrollToStart();" else "readium.scrollToEnd();"
        neighbour.evaluateJavascript(js, null)
    }

    /** The classic rubber-band curve: travel beyond an edge approaches
     *  but never reaches half a page, with the familiar 0.55 resistance. */
    private fun rubberBand(excess: Float): Float {
        val limit = width / 2f
        if (limit <= 0f || excess == 0f) return 0f
        val pulled = abs(excess) * 0.55f
        return pulled * limit / (pulled + limit) * (if (excess < 0) -1f else 1f)
    }

    // MARK: Release

    private fun finishGesture(cancelled: Boolean) {
        dragging = false
        val tracker = velocityTracker
        tracker?.computeCurrentVelocity(1000)
        val fingerVelocity = if (cancelled) 0f else tracker?.getXVelocity(activePointerId) ?: 0f
        // Strip space runs against the finger.
        val contentVelocity = -fingerVelocity
        activePointerId = MotionEvent.INVALID_POINTER_ID

        val currentPager = pager
        when {
            currentPager != null && currentPager.isFakeDragging && abs(boundaryPx) > 0.5f -> {
                val sign = if (boundaryPx > 0f) 1 else -1
                val commits = !cancelled && boundaryCommits(boundaryPx, contentVelocity)
                settlePager(
                    target = basePagerScrollX + (if (commits) sign * width else 0),
                    velocity = contentVelocity,
                    kind = when {
                        !commits -> Settle.PAGER_RETURN
                        sign > 0 -> Settle.PAGER_COMMIT_PLUS
                        else -> Settle.PAGER_COMMIT_MINUS
                    },
                )
            }
            abs(rubberRaw) > 0.5f -> settleRubber()
            else -> settleInner(contentVelocity)
        }
    }

    /** Shared with iOS: a flick decides by its direction alone; a plain
     *  release commits at a third of the page. */
    private fun boundaryCommits(displacement: Float, velocity: Float): Boolean {
        if (abs(velocity) >= minFlingVelocityPx) {
            return (velocity > 0f) == (displacement > 0f)
        }
        return abs(displacement) >= width / 3f
    }

    private fun settlePager(target: Int, velocity: Float, kind: Settle) {
        val currentPager = pager ?: return
        settle = kind
        spring.start(
            from = currentPager.scrollX.toFloat(),
            velocity = velocity,
            target = target.toFloat(),
            onFrame = { position, springVelocity ->
                val live = pager
                if (live != null && live.isFakeDragging && live.isAttachedToWindow) {
                    live.fakeDragBy(live.scrollX - position)
                    boundaryPx = position - basePagerScrollX
                    feedSpringVelocity(live, springVelocity)
                    true
                } else {
                    false
                }
            },
            onSettle = {
                // Landing exactly on a page offset at ~zero velocity:
                // ViewPager's own release computation deterministically
                // keeps this page, and `onPageSelected` runs Readium's
                // bookkeeping exactly as for a programmatic turn.
                val committed = if (kind != Settle.PAGER_RETURN) preRastered else null
                val positionedX = committed?.scrollX ?: 0
                pager?.takeIf { it.isFakeDragging }?.endFakeDrag()
                // One piece of that bookkeeping is wrong for us in RTL:
                // `onPageSelected` re-seats the committed resource by
                // *physical* column index chosen from the *logical*
                // direction (`setCurrentItem(0)` going forward), and in
                // RTL physical column 0 is the chapter's LAST page.
                // Readium never hits this itself (its EPUB pager refuses
                // native touch; programmatic turns take the RTL-aware
                // `goToNextResource`), but a fake-drag commit does. The
                // neighbour was pre-positioned correctly before it slid
                // in, so restore that offset; synchronous, no flash.
                if (committed != null && committed.scrollX != positionedX) {
                    committed.scrollTo(positionedX, committed.scrollY)
                }
                settleDone()
            },
            onAbort = { abortSettle() },
        )
    }

    private fun settleRubber() {
        settle = Settle.RUBBER
        spring.start(
            from = childTranslationX(),
            velocity = 0f,
            target = 0f,
            onFrame = { position, _ ->
                setChildTranslationX(position)
                true
            },
            onSettle = {
                rubberRaw = 0f
                settleDone()
            },
            onAbort = { abortSettle() },
        )
    }

    private fun settleInner(velocity: Float) {
        val visible = webView ?: run { settleDone(); return }
        val pitch = innerPitch
        if (pitch <= 0f) {
            settleDone()
            return
        }
        val offset = visible.scrollX.toFloat()
        val page = offset / pitch
        val targetPage = if (abs(velocity) >= minFlingVelocityPx) {
            // A flick always advances to the next page boundary in its
            // direction.
            if (velocity > 0f) kotlin.math.floor(page).toInt() + 1 else kotlin.math.ceil(page).toInt() - 1
        } else {
            val travel = page - startPage
            val whole = travel.toInt()
            val fraction = travel - whole
            startPage + whole + when {
                abs(fraction) < COMMIT_FRACTION -> 0
                fraction > 0 -> 1
                else -> -1
            }
        }
        val maxPage = if (innerMax == Int.MAX_VALUE) {
            (page.roundToInt() + 1)
        } else {
            (innerMax / pitch).roundToInt()
        }
        val ceiling = if (innerMax == Int.MAX_VALUE) Float.MAX_VALUE else innerMax.toFloat()
        val target = (targetPage.coerceIn(0, maxPage) * pitch).coerceIn(0f, ceiling)
        startInnerSpring(offset, velocity, target)
    }

    private fun startInnerSpring(from: Float, velocity: Float, target: Float) {
        settle = Settle.INNER
        spring.start(
            from = from,
            velocity = velocity,
            target = target,
            onFrame = { position, springVelocity ->
                val live = webView
                if (live != null && live.isAttachedToWindow) {
                    live.scrollTo(position.roundToInt(), live.scrollY)
                    feedSpringVelocity(live, springVelocity)
                    true
                } else {
                    false
                }
            },
            onSettle = { settleDone() },
            onAbort = { abortSettle() },
        )
    }

    /** A settle whose drive surface died mid-flight (detached view, the
     *  toolkit rebuilding its pager): tear the state down anyway, or the
     *  fake drag, pre-raster, and settle kind all leak. */
    private fun abortSettle() {
        settle = Settle.NONE
        settleDone()
    }

    private fun settleDone() {
        settle = Settle.NONE
        preRastered?.settings?.offscreenPreRaster = false
        preRastered = null
        boundaryPx = 0f
        rubberRaw = 0f
        reset()
    }

    /** Bare state reset; any stranded fake drag is closed without acting. */
    private fun reset() {
        if (!spring.isRunning) {
            closeFakeDrag()
        }
        dragging = false
        activePointerId = MotionEvent.INVALID_POINTER_ID
        velocityTracker?.recycle()
        velocityTracker = null
    }

    /**
     * Ends a live fake drag *without* turning a page: `endFakeDrag`
     * always runs ViewPager's release computation, so ending it at an
     * arbitrary mid-boundary offset would commit the neighbour and fire
     * `onPageSelected` — racing whatever navigation interrupted us.
     * Driving back to the base offset first makes the end a no-op.
     */
    private fun closeFakeDrag() {
        val currentPager = pager?.takeIf { it.isFakeDragging } ?: return
        runCatching {
            currentPager.fakeDragBy((currentPager.scrollX - basePagerScrollX).toFloat())
            currentPager.endFakeDrag()
        }
    }

    /** The WebView showing the current page, for the shell's own JS probes. */
    fun currentWebView(): WebView? = navigator?.view?.let(::visibleWebView)

    /**
     * The page reflowed under us (a user-CSS change). Drop every cached
     * measurement so the next claim re-derives the column grid from the
     * new layout, standing any interaction down first. `rejected` is left
     * set by the cancel, which is harmless — it resets on the next
     * ACTION_DOWN.
     */
    fun recalibrate() {
        cancelInteraction()
        pitchSource = null
        innerPitch = 0f
        innerMax = Int.MAX_VALUE
        innerMaxGeneration++
        webView = null
    }

    /** Cancels everything mid-flight — teardown, jumps, detach. */
    fun cancelInteraction() {
        spring.cancel()
        settle = Settle.NONE
        frozen = Settle.NONE
        closeFakeDrag()
        setChildTranslationX(0f)
        preRastered?.settings?.offscreenPreRaster = false
        preRastered = null
        boundaryPx = 0f
        rubberRaw = 0f
        dragging = false
        rejected = true
        activePointerId = MotionEvent.INVALID_POINTER_ID
        velocityTracker?.recycle()
        velocityTracker = null
    }

    // MARK: Programmatic turns (edge taps, keys)

    /** Turns toward the next page in reading order. */
    fun turnForward(): Boolean = turnLogical(forward = true)

    /** Turns toward the previous page in reading order. */
    fun turnBackward(): Boolean = turnLogical(forward = false)

    private fun turnLogical(forward: Boolean): Boolean {
        val nav = navigator ?: return false
        val rtl = nav.overflow.value.readingProgression == ReadingProgression.RTL
        return turnGeometric(if (forward != rtl) 1 else -1)
    }

    /** Turns toward +x (sign 1) or -x (sign -1) — the geometric sides,
     *  whatever the reading progression. */
    fun turnGeometric(sign: Int): Boolean {
        val nav = navigator ?: return false
        if (nav.overflow.value.scroll || dragging) return false
        // A turn already in flight: successive taps chain by moving the
        // running spring's goal one page further, staying inside the
        // resource; a running boundary turn takes the tap as seen.
        if (spring.isRunning) {
            if (settle == Settle.INNER && innerPitch > 0f) {
                val next = spring.currentTarget + sign * innerPitch
                val ceiling = if (innerMax == Int.MAX_VALUE) Float.MAX_VALUE else innerMax.toFloat()
                if (next in 0f..ceiling) spring.retarget(next)
            }
            return true
        }
        val root = nav.view ?: return false
        val visible = visibleWebView(root) ?: return false
        webView = visible
        if (visible.width <= 0) return false
        onTurnGesture?.invoke()

        if (visible.canScrollHorizontally(sign)) {
            seedInnerMax(visible)
            val pitch = innerPitch
            if (pitch <= 0f) return false
            startPage = (visible.scrollX / pitch).roundToInt()
            val ceiling = if (innerMax == Int.MAX_VALUE) Float.MAX_VALUE else innerMax.toFloat()
            val target = ((startPage + sign) * pitch).coerceIn(0f, ceiling)
            startInnerSpring(visible.scrollX.toFloat(), 0f, target)
            return true
        }

        // A boundary turn: slide the neighbour in with the same spring a
        // drag-release uses, then commit through the pager's own settle.
        val foundPager = pagerIn(root)
        pager = foundPager
        neighbourReadyPlus = 0
        neighbourReadyMinus = 0
        basePagerScrollX = foundPager?.scrollX ?: 0
        if (foundPager != null && neighbourReady(sign)) {
            settlePager(
                target = basePagerScrollX + sign * width,
                velocity = 0f,
                kind = if (sign > 0) Settle.PAGER_COMMIT_PLUS else Settle.PAGER_COMMIT_MINUS,
            )
            return true
        }
        // Honest fallback when the neighbour cannot slide (still loading,
        // or the hierarchy changed shape): an instant, unanimated turn.
        val rtl = nav.overflow.value.readingProgression == ReadingProgression.RTL
        val forward = (sign > 0) != rtl
        if (forward) nav.goForward(animated = false) else nav.goBackward(animated = false)
        return true
    }

    // MARK: Frame-rate hints

    /**
     * Adaptive-refresh displays pick their rate partly from content
     * velocity, and the hint resets every frame — feed the finger's
     * instantaneous speed while it drives the strip, and the spring's
     * while one settles. The old settle "pump" died with the takeover:
     * our spring is the animator now, so the hint rides it directly.
     */
    private fun feedFingerVelocity(x: Float, eventTime: Long) {
        if (Build.VERSION.SDK_INT < 35) return
        val dt = eventTime - lastMoveUptime
        if (dt > 0) {
            val speed = abs(x - lastX) / dt * 1000f
            pager?.frameContentVelocity = speed
            webView?.frameContentVelocity = speed
        }
        lastMoveUptime = eventTime
    }

    private fun feedSpringVelocity(view: View, velocity: Float) {
        if (Build.VERSION.SDK_INT >= 35) {
            view.frameContentVelocity = abs(velocity)
        }
    }

    // MARK: Child transforms

    private fun childTranslationX(): Float = getChildAt(0)?.translationX ?: 0f

    private fun setChildTranslationX(value: Float) {
        getChildAt(0)?.translationX = value
    }

    // MARK: View-hierarchy probes

    // The pager keeps neighbouring resources instantiated, so several
    // WebViews coexist; the visible one covers the most screen and
    // neighbours sit beside it in layout space.

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

    private companion object {
        /** A release at this speed reads as a flick — matched to iOS. */
        const val MIN_FLING_VELOCITY_DP_S = 300f

        /** A turn commits at a third of the page — matched to iOS. */
        const val COMMIT_FRACTION = 1f / 3f
    }
}
