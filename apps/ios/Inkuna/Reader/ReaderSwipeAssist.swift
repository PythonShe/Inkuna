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
@MainActor
final class ReaderSwipeAssist: NSObject, UIGestureRecognizerDelegate {
    private weak var navigator: EPUBNavigatorViewController?

    init(navigator: EPUBNavigatorViewController) {
        self.navigator = navigator
        super.init()
        for direction in [UISwipeGestureRecognizer.Direction.left, .right] {
            let recognizer = UISwipeGestureRecognizer(target: self, action: #selector(swiped))
            recognizer.direction = direction
            // Purely observational: the scroll views keep the touch stream.
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

    @objc private func swiped(_ recognizer: UISwipeGestureRecognizer) {
        guard
            let navigator,
            // Vertical-writing publications page on the other axis.
            navigator.presentation.axis == .horizontal,
            let webView = visibleWebView(in: navigator.view),
            let outer = nearestScrollView(above: webView)
        else { return }

        // The outer scroll view picked the pan up (the inner was settled at
        // its edge), so the native resource turn is already under way.
        if outer.isTracking || outer.isDragging || outer.isDecelerating { return }

        let inner = webView.scrollView
        let pageWidth = inner.bounds.width
        guard pageWidth > 0 else { return }

        // Geometric, not logical: a leftward swipe always asks for more
        // content on the right, whatever the reading progression — which is
        // exactly what `goRight` means, so RTL needs no special casing.
        let remaining = recognizer.direction == .left
            ? inner.contentSize.width - inner.bounds.width - inner.contentOffset.x
            : inner.contentOffset.x

        // Fire only when this swipe's page is the resource's outermost in
        // that direction: settled on it (remaining ≈ 0) or still snapping
        // into it from the previous turn (remaining < one page). Anywhere
        // deeper in the resource the inner scroll view handles the turn
        // itself, and doubling it here would skip a page.
        guard remaining < pageWidth * 0.95 else { return }

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
