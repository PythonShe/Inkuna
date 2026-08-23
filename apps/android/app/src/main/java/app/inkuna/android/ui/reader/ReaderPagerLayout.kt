package app.inkuna.android.ui.reader

import android.annotation.SuppressLint
import android.content.Context
import android.os.Build
import android.view.MotionEvent
import android.view.VelocityTracker
import android.view.View
import android.view.ViewConfiguration
import android.widget.FrameLayout
import kotlin.math.abs
import kotlin.math.ceil
import kotlin.math.floor
import kotlin.math.roundToInt

/**
 * The reader's renderer-neutral page-turn controller.
 *
 * It owns the touch pipeline and spring physics; a [ReaderPagerSurface]
 * supplies the two strips it moves.
 */
class ReaderPagerLayout(context: Context) : FrameLayout(context) {
    var surface: ReaderPagerSurface? = null

    /** Fired when a drag is claimed, before the page begins moving. */
    var onTurnGesture: (() -> Unit)? = null

    /**
     * Fired when a programmatic turn (edge tap, key, accessibility page
     * action) meets a boundary whose neighbour exists but has not finished
     * laying out. The host schedules that chapter and completes the turn
     * on its ready event; the geometric sign is passed through.
     */
    var onBoundaryTurnPending: ((Int) -> Unit)? = null

    private val touchSlop = ViewConfiguration.get(context).scaledTouchSlop
    private val minFlingVelocityPx =
        MIN_FLING_VELOCITY_DP_S * context.resources.displayMetrics.density

    // MARK: Gesture state

    private var activePointerId = MotionEvent.INVALID_POINTER_ID
    private var downX = 0f
    private var downY = 0f
    private var anchorX = 0f
    private var lastX = 0f
    private var lastMoveUptime = 0L
    private var rejected = false
    private var dragging = false
    private var frozen = Settle.NONE
    private var frozenTarget = 0f
    private var frozenVelocity = 0f
    private var velocityTracker: VelocityTracker? = null

    // MARK: Strip state (one interaction)

    private var innerRange: ClosedFloatingPointRange<Float> = 0f..0f
    private var outerRange: ClosedFloatingPointRange<Float> = 0f..0f
    private var innerPitch = 0f
    private var outerPitch = 0f
    private var baseStrip = 0f
    private var outerHome = 0f
    private var startPage = 0
    private var boundaryPx = 0f
    private var rubberRaw = 0f
    private var neighbourReadyPlus = 0
    private var neighbourReadyMinus = 0

    // MARK: Settle state

    private val spring = SettleSpring()
    private enum class Settle { NONE, INNER, PAGER_COMMIT_PLUS, PAGER_COMMIT_MINUS, PAGER_RETURN, RUBBER }
    private var settle = Settle.NONE
    private var pickedUpCommit = false
    private var chainTurnSign = 0
    private var chainTurnVelocity = 0f

    /** Whether the surface is bracketed inside begin/endPagingInteraction. */
    private var surfaceInteractionHeld = false

    private fun beginSurfaceInteraction() {
        if (!surfaceInteractionHeld) {
            surface?.beginPagingInteraction()
            surfaceInteractionHeld = true
        }
    }

    private fun endSurfaceInteraction() {
        if (surfaceInteractionHeld) {
            surfaceInteractionHeld = false
            surface?.endPagingInteraction()
        }
    }

    /** Replaces the renderer surface; the old surface is first returned home. */
    fun bind(surface: ReaderPagerSurface) {
        cancelInteraction()
        this.surface = surface
    }

    // MARK: Touch pipeline

    override fun onInterceptTouchEvent(ev: MotionEvent): Boolean {
        when (ev.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                rejected = false
                activePointerId = ev.getPointerId(0)
                downX = ev.x
                downY = ev.y
                anchorX = ev.x
                lastX = ev.x
                lastMoveUptime = ev.eventTime
                velocityTracker?.recycle()
                velocityTracker = VelocityTracker.obtain().also { it.addMovement(ev) }
                if (spring.isRunning) freezeSettle()
            }
            MotionEvent.ACTION_MOVE -> if (!dragging && !rejected) {
                velocityTracker?.addMovement(ev)
                if (frozen != Settle.NONE) claimFrozen(ev) else considerClaim(ev)
            }
            MotionEvent.ACTION_POINTER_DOWN -> if (!dragging) rejected = true
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> if (!dragging) {
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
                applyStrip(baseStrip + (anchorX - x))
                lastX = x
            }
            MotionEvent.ACTION_POINTER_UP -> if (ev.getPointerId(ev.actionIndex) == activePointerId) {
                finishGesture(cancelled = false)
            }
            MotionEvent.ACTION_UP -> finishGesture(cancelled = false)
            MotionEvent.ACTION_CANCEL -> finishGesture(cancelled = true)
        }
        return true
    }

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
        if (abs(dy) > touchSlop && abs(dy) > abs(dx)) {
            rejected = true
            return
        }
        if (abs(dx) <= touchSlop || abs(dx) < abs(dy)) return

        val currentSurface = surface ?: run { rejected = true; return }
        if (!currentSurface.isEngageable || currentSurface.isBusy ||
            currentSurface.hasActiveSelection || SelectionModeTracker.startedSince(ev.downTime)
        ) {
            rejected = true
            return
        }
        beginSurfaceInteraction()
        val inner = currentSurface.innerMetrics()
        val outer = currentSurface.outerMetrics()
        if (inner == null || outer == null || inner.pageWidth <= 0f) {
            endSurfaceInteraction()
            rejected = true
            return
        }

        innerRange = inner.range
        outerRange = outer.range
        innerPitch = inner.pageWidth
        outerPitch = outer.pageWidth
        baseStrip = inner.offset
        outerHome = outer.offset
        anchorX = ev.getX(index)
        startPage = (inner.offset / innerPitch).roundToInt()
        boundaryPx = 0f
        rubberRaw = 0f
        neighbourReadyPlus = 0
        neighbourReadyMinus = 0
        pickedUpCommit = false
        dragging = true
        onTurnGesture?.invoke()
    }

    private fun freezeSettle() {
        frozen = settle
        frozenTarget = spring.currentTarget
        frozenVelocity = spring.currentVelocity
        spring.cancel()
        settle = Settle.NONE
    }

    private fun claimFrozen(ev: MotionEvent) {
        val index = ev.findPointerIndex(activePointerId)
        if (index < 0 || abs(ev.getX(index) - downX) <= touchSlop) return
        val currentSurface = surface ?: run {
            frozen = Settle.NONE
            rejected = true
            endSurfaceInteraction()
            return
        }
        when (val kind = frozen) {
            Settle.PAGER_COMMIT_PLUS, Settle.PAGER_COMMIT_MINUS, Settle.PAGER_RETURN -> {
                val inner = currentSurface.innerMetrics()
                val outer = currentSurface.outerMetrics()
                if (inner == null || outer == null) {
                    frozen = Settle.NONE
                    rejected = true
                    endSurfaceInteraction()
                    return
                }
                boundaryPx = outer.offset - outerHome
                baseStrip = inner.offset + boundaryPx
                pickedUpCommit = kind != Settle.PAGER_RETURN
            }
            Settle.INNER -> {
                val inner = currentSurface.innerMetrics() ?: run {
                    frozen = Settle.NONE
                    rejected = true
                    endSurfaceInteraction()
                    return
                }
                baseStrip = inner.offset
                startPage = (inner.offset / inner.pageWidth).roundToInt()
            }
            Settle.RUBBER -> {
                val inner = currentSurface.innerMetrics() ?: run {
                    frozen = Settle.NONE
                    rejected = true
                    endSurfaceInteraction()
                    return
                }
                val outer = currentSurface.outerMetrics()
                baseStrip = inner.offset + ((outer?.offset ?: outerHome) - outerHome)
            }
            Settle.NONE -> return
        }
        frozen = Settle.NONE
        anchorX = ev.getX(index)
        dragging = true
        onTurnGesture?.invoke()
    }

    private fun resumeFrozen() {
        val kind = frozen
        frozen = Settle.NONE
        when (kind) {
            Settle.INNER -> surface?.innerMetrics()?.let {
                startInnerSpring(it.offset, frozenVelocity, frozenTarget)
            } ?: cancelInteraction()
            Settle.PAGER_COMMIT_PLUS, Settle.PAGER_COMMIT_MINUS, Settle.PAGER_RETURN ->
                settlePager(frozenTarget, frozenVelocity, kind)
            Settle.RUBBER -> settleRubber()
            Settle.NONE -> Unit
        }
    }

    // MARK: The strip

    private fun applyStrip(strip: Float) {
        val currentSurface = surface ?: return
        // On the engine path a chapter's published page count grows during
        // layout; adopt mid-gesture growth (the surface only ever reports it
        // when no offset moves) so the drag reaches pages published under
        // the finger instead of spilling into a chapter crossing.
        currentSurface.innerMetrics()?.let { fresh ->
            if (fresh.pageWidth == innerPitch && fresh.range.endInclusive > innerRange.endInclusive) {
                innerRange = innerRange.start..fresh.range.endInclusive
            }
        }
        val innerTarget = strip.coerceIn(innerRange.start, innerRange.endInclusive)
        val overflow = strip - innerTarget
        currentSurface.setInnerOffset(innerTarget)

        if (overflow != 0f && neighbourReady(if (overflow > 0f) 1 else -1)) {
            boundaryPx = overflow.coerceIn(-outerPitch, outerPitch)
            rubberRaw = 0f
            currentSurface.setOuterOffset(outerHome + boundaryPx)
        } else {
            boundaryPx = 0f
            rubberRaw = overflow
            currentSurface.setOuterOffset(outerHome + rubberBand(overflow))
        }
    }

    /** Whether a neighbour chapter exists on this side of the outer range. */
    private fun neighbourExists(sign: Int): Boolean =
        if (sign > 0) outerRange.endInclusive > outerHome else outerRange.start < outerHome

    private fun neighbourReady(sign: Int): Boolean {
        val inRange = if (sign > 0) {
            outerHome + outerPitch <= outerRange.endInclusive + 0.5f
        } else {
            outerHome - outerPitch >= outerRange.start - 0.5f
        }
        if (!inRange) return false
        val verdict = if (sign > 0) neighbourReadyPlus else neighbourReadyMinus
        if (verdict != 0) return verdict > 0
        val ready = surface?.neighborIsReady(sign > 0) == true
        if (sign > 0) neighbourReadyPlus = if (ready) 1 else -1
        else neighbourReadyMinus = if (ready) 1 else -1
        return ready
    }

    /** The classic rubber-band curve: unchanged shared reader feel. */
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
        val contentVelocity = -fingerVelocity
        activePointerId = MotionEvent.INVALID_POINTER_ID

        chainTurnSign = 0
        when {
            abs(boundaryPx) > 0.5f -> {
                val sign = if (boundaryPx > 0f) 1 else -1
                val commits = !cancelled && boundaryCommits(boundaryPx, contentVelocity)
                if (commits && pickedUpCommit &&
                    abs(contentVelocity) >= minFlingVelocityPx &&
                    (contentVelocity > 0f) == (sign > 0)
                ) {
                    chainTurnSign = sign
                    chainTurnVelocity = contentVelocity
                }
                settlePager(
                    target = outerHome + if (commits) sign * outerPitch else 0f,
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
        pickedUpCommit = false
    }

    private fun boundaryCommits(displacement: Float, velocity: Float): Boolean {
        if (abs(velocity) >= minFlingVelocityPx) return (velocity > 0f) == (displacement > 0f)
        return abs(displacement) >= width / 3f
    }

    private fun settlePager(target: Float, velocity: Float, kind: Settle) {
        val currentSurface = surface ?: run { settleDone(); return }
        val from = currentSurface.outerMetrics()?.offset ?: run { settleDone(); return }
        settle = kind
        val commitDir = when (kind) {
            Settle.PAGER_COMMIT_PLUS -> 1f
            Settle.PAGER_COMMIT_MINUS -> -1f
            else -> 0f
        }
        spring.start(
            from = from,
            velocity = velocity,
            target = target,
            onFrame = { position, springVelocity ->
                val live = surface
                if (live != null && !live.isBusy) {
                    if (commitDir != 0f && (position - target) * commitDir >= -COMMIT_LAND_DISTANCE_PX) {
                        spring.cancel()
                        live.setOuterOffset(target)
                        boundaryPx = target - outerHome
                        landPagerSettle(kind)
                        false
                    } else {
                        live.setOuterOffset(position)
                        boundaryPx = position - outerHome
                        feedSpringVelocity(this, springVelocity)
                        true
                    }
                } else {
                    false
                }
            },
            onSettle = { landPagerSettle(kind) },
            onAbort = { abortSettle() },
        )
    }

    private fun landPagerSettle(kind: Settle) {
        val moved = if (kind == Settle.PAGER_RETURN) {
            true
        } else {
            surface?.commitBoundaryCrossing(kind == Settle.PAGER_COMMIT_PLUS) == true
        }
        settleDone()
        if (moved && kind != Settle.PAGER_RETURN && chainTurnSign != 0) {
            val chained = chainTurnSign
            val chainedVelocity = chainTurnVelocity
            chainTurnSign = 0
            chainTurnVelocity = 0f
            post { turnGeometric(chained, chainedVelocity) }
        } else if (!moved) {
            chainTurnSign = 0
            chainTurnVelocity = 0f
        }
    }

    private fun settleRubber() {
        val from = surface?.outerMetrics()?.offset ?: run { settleDone(); return }
        settle = Settle.RUBBER
        spring.start(
            from = from,
            velocity = 0f,
            target = outerHome,
            onFrame = { position, _ ->
                surface?.setOuterOffset(position)
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
        val inner = surface?.innerMetrics() ?: run { settleDone(); return }
        val pitch = inner.pageWidth
        if (pitch <= 0f) {
            settleDone()
            return
        }
        val offset = inner.offset
        val page = offset / pitch
        val targetPage = if (abs(velocity) >= minFlingVelocityPx) {
            if (velocity > 0f) floor(page).toInt() + 1 else ceil(page).toInt() - 1
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
        val maxPage = (inner.range.endInclusive / pitch).roundToInt()
        if (abs(velocity) >= minFlingVelocityPx) {
            val sign = if (velocity > 0f) 1 else -1
            if ((sign > 0 && targetPage > maxPage) || (sign < 0 && targetPage < 0)) {
                if (startBoundaryFlight(sign, velocity)) return
            }
        }
        val target = (targetPage.coerceIn(0, maxPage) * pitch)
            .coerceIn(inner.range.start, inner.range.endInclusive)
        startInnerSpring(offset, velocity, target)
    }

    private fun startBoundaryFlight(sign: Int, velocity: Float): Boolean {
        if (!neighbourReady(sign)) return false
        settlePager(
            target = outerHome + sign * outerPitch,
            velocity = velocity,
            kind = if (sign > 0) Settle.PAGER_COMMIT_PLUS else Settle.PAGER_COMMIT_MINUS,
        )
        return true
    }

    private fun startInnerSpring(from: Float, velocity: Float, target: Float) {
        settle = Settle.INNER
        spring.start(
            from = from,
            velocity = velocity,
            target = target,
            onFrame = { position, springVelocity ->
                val live = surface
                if (live != null && !live.isBusy) {
                    live.setInnerOffset(position)
                    feedSpringVelocity(this, springVelocity)
                    true
                } else {
                    false
                }
            },
            onSettle = { settleDone() },
            onAbort = { abortSettle() },
        )
    }

    private fun abortSettle() {
        settle = Settle.NONE
        chainTurnSign = 0
        settleDone()
    }

    private fun settleDone() {
        settle = Settle.NONE
        boundaryPx = 0f
        rubberRaw = 0f
        surface?.setOuterOffset(outerHome)
        reset()
        endSurfaceInteraction()
    }

    private fun reset() {
        dragging = false
        activePointerId = MotionEvent.INVALID_POINTER_ID
        velocityTracker?.recycle()
        velocityTracker = null
    }

    // MARK: Programmatic turns

    fun turnForward(): Boolean = turnLogical(forward = true)

    fun turnBackward(): Boolean = turnLogical(forward = false)

    private fun turnLogical(forward: Boolean): Boolean {
        val currentSurface = surface ?: return false
        return turnGeometric(if (forward != currentSurface.isRightToLeft) 1 else -1)
    }

    fun turnGeometric(sign: Int, velocity: Float = 0f): Boolean {
        val currentSurface = surface ?: return false
        if (!currentSurface.isEngageable || currentSurface.isBusy || dragging) return false
        var didAbortSettle = false
        if (spring.isRunning) {
            when (settle) {
                Settle.INNER -> {
                    val next = spring.currentTarget + sign * innerPitch
                    if (next in innerRange.start..innerRange.endInclusive) spring.retarget(next)
                    return true
                }
                Settle.PAGER_RETURN, Settle.RUBBER -> {
                    spring.cancel()
                    surface?.setOuterOffset(outerHome)
                    settleDone()
                    didAbortSettle = true
                }
                else -> return true
            }
        }

        beginSurfaceInteraction()
        val inner = currentSurface.innerMetrics() ?: run { endSurfaceInteraction(); return didAbortSettle }
        val outer = currentSurface.outerMetrics() ?: run { endSurfaceInteraction(); return didAbortSettle }
        if (inner.pageWidth <= 0f) {
            endSurfaceInteraction()
            return didAbortSettle
        }
        onTurnGesture?.invoke()
        innerRange = inner.range
        outerRange = outer.range
        innerPitch = inner.pageWidth
        outerPitch = outer.pageWidth
        outerHome = outer.offset
        startPage = (inner.offset / innerPitch).roundToInt()
        baseStrip = inner.offset
        neighbourReadyPlus = 0
        neighbourReadyMinus = 0

        val target = inner.offset + sign * innerPitch
        if (target in inner.range.start..inner.range.endInclusive) {
            startInnerSpring(inner.offset, velocity, target)
            return true
        }
        if (neighbourReady(sign)) {
            settlePager(
                target = outerHome + sign * outerPitch,
                velocity = velocity,
                kind = if (sign > 0) Settle.PAGER_COMMIT_PLUS else Settle.PAGER_COMMIT_MINUS,
            )
            return true
        }
        if (neighbourExists(sign)) {
            // The neighbour is still laying out: hand the turn to the host,
            // which schedules the chapter and completes it on readiness.
            onBoundaryTurnPending?.let { pending ->
                endSurfaceInteraction()
                pending(sign)
                return true
            }
        }
        endSurfaceInteraction()
        return didAbortSettle
    }

    /** Cancels every in-flight interaction for teardown, jumps, and reflow. */
    fun cancelInteraction() {
        spring.cancel()
        settle = Settle.NONE
        frozen = Settle.NONE
        chainTurnSign = 0
        pickedUpCommit = false
        boundaryPx = 0f
        rubberRaw = 0f
        // Close the outer strip against its live geometry — the surface's
        // home is its current page width by contract — instead of replaying
        // the home captured at claim, which goes stale when the width
        // changes (rotation) between claim and cancel.
        surface?.let { live -> live.outerMetrics()?.let { live.setOuterOffset(it.pageWidth) } }
        dragging = false
        rejected = true
        activePointerId = MotionEvent.INVALID_POINTER_ID
        velocityTracker?.recycle()
        velocityTracker = null
        endSurfaceInteraction()
    }

    // MARK: Frame-rate hints

    private fun feedFingerVelocity(x: Float, eventTime: Long) {
        if (Build.VERSION.SDK_INT < 35) return
        val dt = eventTime - lastMoveUptime
        if (dt > 0) frameContentVelocity = abs(x - lastX) / dt * 1000f
        lastMoveUptime = eventTime
    }

    private fun feedSpringVelocity(view: View, velocity: Float) {
        if (Build.VERSION.SDK_INT >= 35) view.frameContentVelocity = abs(velocity)
    }

    private companion object {
        const val MIN_FLING_VELOCITY_DP_S = 300f
        const val COMMIT_FRACTION = 1f / 3f
        const val COMMIT_LAND_DISTANCE_PX = 3f
    }
}
