import UIKit

/// Motion tokens ported from the Inkuna design system (`tokens/motion.css`).
///
/// Two curves, three durations. `quiet` is for fades and color changes;
/// `page` settles without bouncing and is reserved for page turns and
/// sheets. Anything interruptible should run through a
/// `UIViewPropertyAnimator` built here.
enum InkMotion {
    /// `--dur-fast` — control feedback.
    static let fast: TimeInterval = 0.18
    /// `--dur-med` — chrome fades, theme cross-fades.
    static let medium: TimeInterval = 0.32
    /// `--dur-slow` — progress fills, page settles.
    static let slow: TimeInterval = 0.56

    /// `--ease-quiet` — cubic-bezier(.4, 0, .2, 1).
    @MainActor
    static var quiet: UITimingCurveProvider {
        UICubicTimingParameters(
            controlPoint1: CGPoint(x: 0.4, y: 0),
            controlPoint2: CGPoint(x: 0.2, y: 1)
        )
    }

    /// `--ease-page` — cubic-bezier(.22, 1, .36, 1); settles, never bounces.
    @MainActor
    static var page: UITimingCurveProvider {
        UICubicTimingParameters(
            controlPoint1: CGPoint(x: 0.22, y: 1),
            controlPoint2: CGPoint(x: 0.36, y: 1)
        )
    }

    /// Interruptible animator on the quiet curve.
    @MainActor
    static func quietAnimator(duration: TimeInterval = medium) -> UIViewPropertyAnimator {
        UIViewPropertyAnimator(duration: duration, timingParameters: quiet)
    }

    /// Interruptible animator on the page curve.
    @MainActor
    static func pageAnimator(duration: TimeInterval = slow) -> UIViewPropertyAnimator {
        UIViewPropertyAnimator(duration: duration, timingParameters: page)
    }

    /// Convenience fade/transform run on the quiet curve.
    @MainActor
    static func runQuiet(
        duration: TimeInterval = medium,
        animations: @escaping @MainActor () -> Void,
        completion: (@MainActor () -> Void)? = nil
    ) {
        let animator = quietAnimator(duration: duration)
        animator.addAnimations(animations)
        if let completion {
            animator.addCompletion { _ in completion() }
        }
        animator.startAnimation()
    }
}
