package app.inkuna.android.ui.reader

import android.view.Choreographer
import kotlin.math.abs

/**
 * A critically damped spring driving one scalar through per-frame
 * callbacks on the [Choreographer]. Critical damping is the page-turn
 * feel: the fastest settle with no overshoot, and interruptible at any
 * frame because true position and velocity are integrated, not sampled
 * off a curve. Nothing here knows about Readium — this physics layer
 * carries over unchanged when the rendering stack is replaced, and its
 * constants are shared with the iOS shell so the two readers feel like
 * one product.
 */
class SettleSpring {
    private var position = 0f
    private var velocity = 0f
    private var target = 0f
    private var lastFrameNanos = 0L
    private var onFrame: ((position: Float, velocity: Float) -> Boolean)? = null
    private var onSettle: (() -> Unit)? = null
    private var onAbort: (() -> Unit)? = null

    private val callback = object : Choreographer.FrameCallback {
        override fun doFrame(frameTimeNanos: Long) {
            if (onFrame == null) return
            // The frame interval from the Choreographer itself, clamped so
            // a hitch cannot integrate a huge step.
            val dt = if (lastFrameNanos == 0L) {
                1f / 120f
            } else {
                ((frameTimeNanos - lastFrameNanos) / 1_000_000_000f).coerceIn(1f / 240f, 1f / 30f)
            }
            lastFrameNanos = frameTimeNanos
            // Semi-implicit Euler: unconditionally stable at any frame
            // rate the dt clamp allows.
            val acceleration = OMEGA * OMEGA * (target - position) - 2f * OMEGA * velocity
            velocity += acceleration * dt
            position += velocity * dt
            if (abs(position - target) < REST_DISTANCE_PX && abs(velocity) < REST_VELOCITY_PX_S) {
                position = target
                val frame = onFrame
                val settle = onSettle
                cancel()
                frame?.invoke(position, 0f)
                settle?.invoke()
                return
            }
            if (onFrame?.invoke(position, velocity) == true) {
                Choreographer.getInstance().postFrameCallback(this)
            } else {
                // The drive surface refused the frame (detached, drag
                // ended under us): the flight cannot land, but its owner
                // still needs to tear its state down.
                val abort = onAbort
                cancel()
                abort?.invoke()
            }
        }
    }

    val isRunning: Boolean get() = onFrame != null

    /** The goal the running spring is heading for. */
    val currentTarget: Float get() = target

    /** The integrated velocity right now — what a freeze must remember
     *  for its momentum to survive the resume. */
    val currentVelocity: Float get() = if (isRunning) velocity else 0f

    /**
     * Starts (or restarts) the spring. [onFrame] returns whether the
     * spring may keep running; [onSettle] fires once, after the final
     * exactly-on-target frame has been delivered. [onAbort] fires
     * instead when a frame is refused and the flight dies mid-air.
     */
    fun start(
        from: Float,
        velocity: Float,
        target: Float,
        onFrame: (position: Float, velocity: Float) -> Boolean,
        onSettle: () -> Unit,
        onAbort: (() -> Unit)? = null,
    ) {
        cancel()
        position = from
        this.velocity = velocity
        this.target = target
        this.onFrame = onFrame
        this.onSettle = onSettle
        this.onAbort = onAbort
        lastFrameNanos = 0L
        Choreographer.getInstance().postFrameCallback(callback)
    }

    /**
     * Moves the goal of a running spring — how successive taps chain
     * turns without restarting the motion.
     */
    fun retarget(newTarget: Float) {
        target = newTarget
    }

    fun cancel() {
        Choreographer.getInstance().removeFrameCallback(callback)
        onFrame = null
        onSettle = null
        onAbort = null
    }

    private companion object {
        /** ω = 20 rad/s ≈ a 0.3 s page settle — matched to iOS. */
        const val OMEGA = 20f

        /** Rest: within half a px, moving slower than 40 px/s. */
        const val REST_DISTANCE_PX = 0.5f
        const val REST_VELOCITY_PX_S = 40f
    }
}
