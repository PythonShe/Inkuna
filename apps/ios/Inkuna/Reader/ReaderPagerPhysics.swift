import QuartzCore
import UIKit

/// The physics half of the pager: a display-link frame loop, a critically
/// damped spring stepped on it, and the pure decision rules for where a
/// released gesture lands. Nothing here knows about Readium — this layer
/// carries over unchanged when the rendering stack is replaced, and its
/// constants are shared with the Android shell so the two readers feel
/// like one product.
enum ReaderPagerRules {
    /// A turn commits at a third of the page — eager, like a physical
    /// page that has clearly left the stack — or on a deliberate flick.
    static let commitFraction: CGFloat = 1.0 / 3.0
    /// Content velocity (pt/s) that reads as a flick. A flick along the
    /// displacement commits regardless of distance; a flick against it
    /// cancels regardless of distance.
    static let flingVelocity: CGFloat = 300

    /// The classic rubber-band curve: travel beyond an edge approaches
    /// but never reaches `limit`, with the familiar 0.55 resistance.
    static func rubberBand(_ excess: CGFloat, limit: CGFloat) -> CGFloat {
        guard limit > 0, excess != 0 else { return 0 }
        let pulled = abs(excess) * 0.55
        return pulled * limit / (pulled + limit) * (excess < 0 ? -1 : 1)
    }

    /// Whether a released boundary drag goes on to the neighbor resource
    /// or snaps back. `displacement` and `velocity` are both in content
    /// coordinates (positive reveals the +x neighbor).
    static func boundaryCommits(displacement: CGFloat, velocity: CGFloat, pageWidth: CGFloat) -> Bool {
        guard pageWidth > 0, displacement != 0 else { return false }
        if abs(velocity) >= flingVelocity {
            return (velocity > 0) == (displacement > 0)
        }
        return abs(displacement) >= pageWidth * commitFraction
    }

    /// The page a released within-resource drag settles on. A flick
    /// always advances to the next page boundary in its direction; a
    /// plain release commits each page the drag carried a third of the
    /// way past, measured from where the gesture began.
    static func innerTargetPage(
        startPage: Int,
        offset: CGFloat,
        velocity: CGFloat,
        pageWidth: CGFloat,
        maxPage: Int
    ) -> Int {
        guard pageWidth > 0, maxPage >= 0 else { return 0 }
        let target: Int
        if abs(velocity) >= flingVelocity {
            let page = offset / pageWidth
            target = velocity > 0
                ? Int(page.rounded(.down)) + 1
                : Int(page.rounded(.up)) - 1
        } else {
            let travel = offset / pageWidth - CGFloat(startPage)
            let whole = travel.rounded(.towardZero)
            let fraction = travel - whole
            target = startPage + Int(whole) +
                (abs(fraction) >= commitFraction ? (fraction > 0 ? 1 : -1) : 0)
        }
        return min(max(target, 0), maxPage)
    }
}

/// A display link wrapped as a per-frame closure. The link rides the
/// panel's full rate (ProMotion; needs CADisableMinimumFrameDurationOnPhone,
/// set in project.yml) and runs in `.common` mode so it keeps firing while
/// a touch is tracking. The closure returns whether to keep running.
///
/// The link retains this object while running, so an owner that lets go
/// mid-flight is kept alive until its closure notices (a weak capture
/// returning false) — no unsafe teardown ordering exists.
@MainActor
final class ReaderPagerFrameLoop {
    private var link: CADisplayLink?
    private var onFrame: ((_ dt: CFTimeInterval) -> Bool)?

    var isRunning: Bool { link != nil }

    func start(_ onFrame: @escaping (_ dt: CFTimeInterval) -> Bool) {
        stop()
        self.onFrame = onFrame
        let link = CADisplayLink(target: self, selector: #selector(tick))
        link.preferredFrameRateRange = CAFrameRateRange(minimum: 30, maximum: 120, preferred: 120)
        link.add(to: .main, forMode: .common)
        self.link = link
    }

    func stop() {
        link?.invalidate()
        link = nil
        onFrame = nil
    }

    @objc private func tick(_ link: CADisplayLink) {
        guard link === self.link else {
            // A superseded link that ticks once more must not outlive its
            // replacement.
            link.invalidate()
            return
        }
        // The frame interval from the link itself, clamped so a hitch or a
        // background gap cannot integrate a huge step.
        let dt = min(max(link.targetTimestamp - link.timestamp, 1.0 / 240.0), 1.0 / 30.0)
        if onFrame?(dt) != true {
            stop()
        }
    }
}

/// A critically damped spring driving one scalar through per-frame
/// callbacks. Critical damping is the page-turn feel: the fastest settle
/// with no overshoot, and interruptible at any frame because the true
/// position and velocity are integrated, not sampled off a curve.
@MainActor
final class ReaderPagerSpring {
    private let loop = ReaderPagerFrameLoop()
    private var position: CGFloat = 0
    private var velocity: CGFloat = 0
    private var onFrame: ((_ position: CGFloat, _ velocity: CGFloat) -> Bool)?
    private var onSettle: (() -> Void)?

    /// ω = 20 rad/s ≈ a 0.3 s page settle — matched to the Android shell.
    private let omega: CGFloat = 20
    /// Rest thresholds: within half a point moving slower than 40 pt/s.
    private let restDistance: CGFloat = 0.5
    private let restVelocity: CGFloat = 40

    private(set) var target: CGFloat = 0
    var isRunning: Bool { loop.isRunning }

    /// Starts (or restarts) the spring. `onFrame` returns whether the
    /// spring may keep running; `onSettle` fires once, after the final
    /// exactly-on-target frame has been delivered.
    func start(
        from: CGFloat,
        velocity: CGFloat,
        target: CGFloat,
        onFrame: @escaping (_ position: CGFloat, _ velocity: CGFloat) -> Bool,
        onSettle: @escaping () -> Void
    ) {
        position = from
        self.velocity = velocity
        self.target = target
        self.onFrame = onFrame
        self.onSettle = onSettle
        loop.start { [weak self] dt in
            self?.step(dt) ?? false
        }
    }

    /// Moves the goal of a running spring — how successive taps chain
    /// turns without restarting the motion.
    func retarget(_ newTarget: CGFloat) {
        target = newTarget
    }

    func cancel() {
        loop.stop()
        onFrame = nil
        onSettle = nil
    }

    private func step(_ dt: CFTimeInterval) -> Bool {
        // Semi-implicit Euler: unconditionally stable at any frame rate
        // the loop's dt clamp allows.
        let dt = CGFloat(dt)
        let acceleration = omega * omega * (target - position) - 2 * omega * velocity
        velocity += acceleration * dt
        position += velocity * dt
        if abs(position - target) < restDistance, abs(velocity) < restVelocity {
            position = target
            let frame = onFrame
            let settle = onSettle
            cancel()
            _ = frame?(position, 0)
            settle?()
            return false
        }
        return onFrame?(position, velocity) ?? false
    }
}
