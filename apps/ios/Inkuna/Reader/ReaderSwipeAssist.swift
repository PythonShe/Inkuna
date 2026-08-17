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
@MainActor
final class ReaderSwipeAssist: NSObject, UIGestureRecognizerDelegate {
    private weak var navigator: EPUBNavigatorViewController?

    /// The visible spread's outer (pagination) scroll view and per-direction
    /// eligibility, captured by the touch-down recognizer.
    private weak var outerScrollView: UIScrollView?
    private var eligibleLeft = false
    private var eligibleRight = false

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

    func gestureRecognizer(
        _ gestureRecognizer: UIGestureRecognizer,
        shouldRecognizeSimultaneouslyWith otherGestureRecognizer: UIGestureRecognizer
    ) -> Bool {
        true
    }

    @objc private func touchedDown(_ recognizer: UILongPressGestureRecognizer) {
        guard recognizer.state == .began else { return }
        eligibleLeft = false
        eligibleRight = false
        outerScrollView = nil
        guard
            let navigator,
            // Vertical-writing publications page on the other axis, and
            // scroll mode has no page snaps to rescue.
            navigator.presentation.axis == .horizontal,
            !navigator.presentation.scroll,
            let webView = visibleWebView(in: navigator.view)
        else { return }
        outerScrollView = nearestScrollView(above: webView)

        let inner = webView.scrollView
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
        guard
            let navigator,
            recognizer.direction == .left ? eligibleLeft : eligibleRight
        else { return }

        // The outer scroll view picked the pan up (the inner was settled at
        // its edge), so the native resource turn is already under way.
        if let outer = outerScrollView,
           outer.isTracking || outer.isDragging || outer.isDecelerating {
            return
        }

        let direction = recognizer.direction
        Task {
            // The navigator's state machine coalesces re-entrant turns, so
            // a second assist landing mid-slide is dropped, not stacked.
            let options = NavigatorGoOptions(animated: true)
            if direction == .left {
                _ = await navigator.goRight(options: options)
            } else {
                _ = await navigator.goLeft(options: options)
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
