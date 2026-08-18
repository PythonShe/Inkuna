import ReadiumNavigator
import UIKit
import WebKit

/// Rescues page swipes that die at a resource (chapter) boundary.
///
/// Readium turns pages within a resource inside the spread web view's own
/// paging scroll view (`bounces` off); crossing into the next resource is
/// the outer pagination scroll view's job, and UIKit only forwards the pan
/// to it once the inner scroll view is fully settled at its edge. A reader
/// swiping at pace therefore hits a dead swipe on every chapter turn: the
/// gesture is captured by the still-settling inner scroll view, which has
/// nowhere left to go, and nothing moves. This recognizer spots exactly
/// that swipe — a fling on the resource's last page while the outer scroll
/// view never picked up the drag — and drives the turn through the
/// navigator instead, so the boundary feels like any other page.
///
/// Eligibility is captured at touch-down, not at swipe recognition: the
/// inner scroll view tracks the finger 1:1, so by recognition time a swipe
/// from the second-to-last page has already dragged it within a page of
/// the edge and a live check would double the native turn. At touch-down
/// the offset is page-aligned unless a previous turn's snap was caught
/// mid-flight — the two cases (settled on the outermost page, frozen while
/// snapping into it) are precisely the ones that need rescuing.
///
/// A second dead-swipe shape exists just *after* a successful resource
/// turn: a touch landing while the outer scroll view is still settling
/// stops the deceleration and hands the whole pan to the outer view, so a
/// flick on the new chapter's first page — a plain inner page turn any
/// other time — moves nothing and snaps back. That case can't be rescued
/// at recognition time (the outer view legitimately owns in-flight turns
/// too), so the swipe instead arms a settle watch: once every scroll view
/// has come to rest, if neither the inner offset nor the outer page moved,
/// the recognized swipe went nowhere and the navigator drives the turn.
@MainActor
final class ReaderSwipeAssist: NSObject, UIGestureRecognizerDelegate {
    private weak var navigator: EPUBNavigatorViewController?

    /// The visible spread's scroll views and per-direction eligibility,
    /// captured by the touch-down recognizer.
    private weak var outerScrollView: UIScrollView?
    private weak var innerScrollView: UIScrollView?
    private var eligibleLeft = false
    private var eligibleRight = false

    /// State for the settle watch: where everything sat at touch-down, and
    /// whether the outer view was caught mid-settle by this touch.
    private var outerWasMidSettle = false
    private var touchDownOuterPage: CGFloat = 0
    private var touchDownInnerOffset: CGFloat = 0
    private var settleWatch: Task<Void, Never>?
    private var directTurnTask: Task<Void, Never>?

    init(navigator: EPUBNavigatorViewController) {
        self.navigator = navigator
        super.init()

        // Fires at touch-down, before any movement; purely observational.
        let touchDown = UILongPressGestureRecognizer(target: self, action: #selector(touchedDown))
        touchDown.minimumPressDuration = 0
        touchDown.allowableMovement = .greatestFiniteMagnitude
        touchDown.cancelsTouchesInView = false
        touchDown.delegate = self
        navigator.view.addGestureRecognizer(touchDown)

        for direction in [UISwipeGestureRecognizer.Direction.left, .right] {
            let recognizer = UISwipeGestureRecognizer(target: self, action: #selector(swiped))
            recognizer.direction = direction
            recognizer.cancelsTouchesInView = false
            recognizer.delegate = self
            navigator.view.addGestureRecognizer(recognizer)
        }
    }

    deinit {
        settleWatch?.cancel()
        directTurnTask?.cancel()
    }

    func gestureRecognizer(
        _ gestureRecognizer: UIGestureRecognizer,
        shouldRecognizeSimultaneouslyWith otherGestureRecognizer: UIGestureRecognizer
    ) -> Bool {
        true
    }

    @objc private func touchedDown(_ recognizer: UILongPressGestureRecognizer) {
        guard recognizer.state == .began else { return }
        // A fresh interaction supersedes any pending settle rescue or in-flight turn.
        settleWatch?.cancel()
        settleWatch = nil
        directTurnTask?.cancel()
        directTurnTask = nil
        eligibleLeft = false
        eligibleRight = false
        outerWasMidSettle = false
        outerScrollView = nil
        innerScrollView = nil
        guard
            let navigator,
            // Vertical-writing publications page on the other axis, and
            // scroll mode has no page snaps to rescue.
            navigator.presentation.axis == .horizontal,
            !navigator.presentation.scroll,
            let webView = visibleWebView(in: navigator.view)
        else { return }
        let outer = nearestScrollView(above: webView)
        outerScrollView = outer
        if let outer, outer.bounds.width > 0 {
            let page = outer.contentOffset.x / outer.bounds.width
            touchDownOuterPage = page.rounded()
            // Decelerating, or frozen off page alignment by this very
            // touch: either way the outer view owns the pan from here.
            outerWasMidSettle = outer.isDecelerating ||
                abs(page - page.rounded()) * outer.bounds.width > 1
        }

        let inner = webView.scrollView
        innerScrollView = inner
        touchDownInnerOffset = inner.contentOffset.x
        let pageWidth = inner.bounds.width
        guard pageWidth > 0 else { return }
        // Geometric, not logical: a leftward swipe always asks for more
        // content on the right, whatever the reading progression — which
        // is exactly what `goRight` means, so RTL needs no special casing.
        let remainingLeft = inner.contentSize.width - inner.bounds.width - inner.contentOffset.x
        let remainingRight = inner.contentOffset.x
        // Under one page of travel left in a direction means this touch's
        // page is the resource's outermost there: at rest the offset is
        // page-aligned (a full page or more remains anywhere deeper), and
        // a mid-snap freeze under a page was already headed to the edge.
        let threshold = pageWidth - 4
        eligibleLeft = remainingLeft < threshold
        eligibleRight = remainingRight < threshold
    }

    @objc private func swiped(_ recognizer: UISwipeGestureRecognizer) {
        guard let navigator else { return }
        let direction = recognizer.direction
        let eligible = direction == .left ? eligibleLeft : eligibleRight
        let outerEngaged = outerScrollView.map {
            $0.isTracking || $0.isDragging || $0.isDecelerating
        } ?? false

        if eligible, !outerEngaged {
            settleWatch?.cancel()
            settleWatch = nil
            directTurnTask?.cancel()
            directTurnTask = Task { [weak self] in
                await self?.turn(direction)
            }
            return
        }
        // The outer scroll view owns the pan — either a native resource
        // turn already under way (which will move the outer page), or a
        // touch that caught the previous turn's settle and is about to
        // snap back having moved nothing. Recognition time can't tell
        // them apart; the settle watch can.
        if outerEngaged || outerWasMidSettle {
            armSettleWatch(direction)
        }
    }

    /// Drives one page turn through the navigator. Its state machine
    /// coalesces re-entrant turns, so a second assist landing mid-slide is
    /// dropped, not stacked.
    private func turn(_ direction: UISwipeGestureRecognizer.Direction) async {
        guard !Task.isCancelled, let navigator else { return }
        let options = NavigatorGoOptions(animated: true)
        if direction == .left {
            _ = await navigator.goRight(options: options)
        } else {
            _ = await navigator.goLeft(options: options)
        }
    }

    /// Waits for every scroll view to come to rest, then turns the page if
    /// the recognized swipe demonstrably moved nothing: the inner offset
    /// and the outer page both sit exactly where they were at touch-down.
    /// Any net movement — an inner column turn, the outer sliding to
    /// another resource — means the gesture (or the turn it interrupted)
    /// worked natively, and the watch ends without acting.
    private func armSettleWatch(_ direction: UISwipeGestureRecognizer.Direction) {
        settleWatch?.cancel()
        directTurnTask?.cancel()
        directTurnTask = nil
        let innerStart = touchDownInnerOffset
        let outerPage = touchDownOuterPage
        settleWatch = Task { [weak self] in
            let deadline = ContinuousClock.now.advanced(by: .seconds(1.5))
            while ContinuousClock.now < deadline {
                try? await Task.sleep(for: .milliseconds(120))
                guard !Task.isCancelled, let self else { return }
                guard let outer = self.outerScrollView,
                      let inner = self.innerScrollView else { return }
                // The recognizing touch may still be down, and a *new*
                // touch cancels this task from touchedDown — so any
                // activity here is this gesture's own; wait it out.
                if outer.isTracking || outer.isDragging || outer.isDecelerating ||
                    inner.isTracking || inner.isDragging || inner.isDecelerating {
                    continue
                }
                let width = outer.bounds.width
                guard width > 0 else { return }
                let page = outer.contentOffset.x / width
                // Readium's programmatic slides animate without
                // decelerating; off page alignment means one is running.
                guard abs(page - page.rounded()) * width < 1 else { continue }

                let outerUnmoved = abs(page.rounded() - outerPage) < 0.5
                let innerUnmoved = abs(inner.contentOffset.x - innerStart) < 1
                if outerUnmoved, innerUnmoved {
                    await self.turn(direction)
                }
                return
            }
        }
    }

    /// The spread web view currently on screen: the one covering the
    /// navigator's center.
    private func visibleWebView(in root: UIView) -> WKWebView? {
        let center = CGPoint(x: root.bounds.midX, y: root.bounds.midY)
        var queue: [UIView] = [root]
        while let view = queue.popLast() {
            if let webView = view as? WKWebView,
               webView.convert(webView.bounds, to: root).contains(center) {
                return webView
            }
            queue.append(contentsOf: view.subviews)
        }
        return nil
    }

    /// The pagination scroll view: the nearest scroll view ancestor of the
    /// spread (the web view's own scroll view sits below it, inside).
    private func nearestScrollView(above view: UIView) -> UIScrollView? {
        var ancestor = view.superview
        while let current = ancestor {
            if let scrollView = current as? UIScrollView { return scrollView }
            ancestor = current.superview
        }
        return nil
    }
}
