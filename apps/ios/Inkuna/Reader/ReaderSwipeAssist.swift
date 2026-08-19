import ReadiumNavigator
import UIKit
import WebKit

/// Rescues page swipes that die around a resource (chapter) boundary.
///
/// Readium turns pages within a resource inside the spread web view's own
/// paging scroll view (`bounces` off); crossing into the next resource is
/// the outer pagination scroll view's job. Two swipe shapes die there:
///
/// 1. A fling on the resource's outermost page while the outer scroll view
///    never picks up the drag — the inner scroll view has nowhere left to
///    go and nothing moves.
/// 2. A flick just *after* a resource turn, landing while the outer scroll
///    view (or the navigator's own slide) is still settling — the touch is
///    consumed by the settle and moves nothing.
///
/// Both are rescued the same way: a recognized horizontal swipe arms a
/// verify task that waits for every scroll view to rest and then checks,
/// against *live, re-resolved* views, that the gesture demonstrably moved
/// nothing. Only then is the missed turn driven — and never through state
/// that can lag reality:
///
/// - Within the resource, the turn is a DOM-relative instant
///   `window.scrollBy` evaluated on the verified web view itself. Readium
///   uses the same primitive for its own turns, but going straight to the
///   web view bypasses the navigator's state machine and its
///   `currentSpreadIndex` fallback, whose bookkeeping commits only when
///   the outer deceleration ends — the lag that used to turn a rescue into
///   a backward jump (`goRight` from a stale index re-entered the visible
///   chapter at `.start`). A relative scroll of an at-rest, page-aligned
///   offset structurally cannot land backward or double.
/// - At a true resource boundary the turn goes through the navigator's
///   `goRight`/`goLeft` (edge-aware, RTL-safe), guarded by the same
///   at-rest checks so the spread index it consults is committed, and
///   unanimated so the slide cannot be caught mid-flight by the next
///   swipe and hand the position to the pager's snap.
///
/// Verification never trusts anything captured at touch-down except the
/// baselines themselves: the web view is re-resolved (the navigator keeps
/// several preloaded, invisible spreads whose model geometry moves before
/// the screen does), a changed web view means the resource turned
/// natively, and the navigator must be idle — read through its own tell,
/// the pagination view's `isUserInteractionEnabled`, which it drops during
/// every programmatic move.
@MainActor
final class ReaderSwipeAssist: NSObject, UIGestureRecognizerDelegate {
    private weak var navigator: EPUBNavigatorViewController?

    /// Baselines captured at touch-down: the spread web view under the
    /// finger, the pagination scroll view, and where both sat.
    private weak var touchDownWebView: WKWebView?
    private weak var outerScrollView: UIScrollView?
    private var touchDownOuterPage: CGFloat = 0
    private var touchDownInnerOffset: CGFloat = 0

    /// Whether this touch could be a dead swipe at all: on the resource's
    /// outermost page in a direction, or interacting with a still-settling
    /// turn. Anything else is a plain page turn the web view handles.
    private var eligibleLeft = false
    private var eligibleRight = false
    private var outerWasBusy = false

    private var verifyTask: Task<Void, Never>?

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
        verifyTask?.cancel()
    }

    func gestureRecognizer(
        _ gestureRecognizer: UIGestureRecognizer,
        shouldRecognizeSimultaneouslyWith otherGestureRecognizer: UIGestureRecognizer
    ) -> Bool {
        true
    }

    @objc private func touchedDown(_ recognizer: UILongPressGestureRecognizer) {
        guard recognizer.state == .began else { return }
        // A fresh interaction supersedes any pending rescue.
        verifyTask?.cancel()
        verifyTask = nil
        eligibleLeft = false
        eligibleRight = false
        outerWasBusy = false
        touchDownWebView = nil
        outerScrollView = nil
        guard
            let navigator,
            // Vertical-writing publications page on the other axis, and
            // scroll mode has no page snaps to rescue.
            navigator.presentation.axis == .horizontal,
            !navigator.presentation.scroll,
            let webView = visibleWebView(in: navigator.view)
        else { return }
        touchDownWebView = webView
        let outer = nearestScrollView(above: webView)
        outerScrollView = outer
        if let outer, outer.bounds.width > 0 {
            let page = outer.contentOffset.x / outer.bounds.width
            touchDownOuterPage = page.rounded()
            // Decelerating, frozen off alignment by this very touch, or
            // locked out by a programmatic slide: this touch is landing on
            // a still-settling turn.
            outerWasBusy = outer.isDecelerating ||
                abs(page - page.rounded()) * outer.bounds.width > 1 ||
                outer.superview?.isUserInteractionEnabled == false
        }

        let inner = webView.scrollView
        touchDownInnerOffset = inner.contentOffset.x
        let pageWidth = inner.bounds.width
        guard pageWidth > 0 else { return }
        // Geometric, not logical: a leftward swipe always asks for more
        // content on the right, whatever the reading progression — which
        // is exactly what `goRight` means, so RTL needs no special casing.
        let remainingLeft = inner.contentSize.width - inner.bounds.width - inner.contentOffset.x
        let remainingRight = inner.contentOffset.x
        // Under one page of travel left in a direction means this touch's
        // page may be the resource's outermost there; verification later
        // re-measures at rest, so a mid-snap over-read only arms the
        // watch, never a turn.
        let threshold = pageWidth - 4
        eligibleLeft = remainingLeft < threshold
        eligibleRight = remainingRight < threshold
    }

    @objc private func swiped(_ recognizer: UISwipeGestureRecognizer) {
        let direction = recognizer.direction
        let eligible = direction == .left ? eligibleLeft : eligibleRight
        let outerEngaged = outerScrollView.map {
            $0.isTracking || $0.isDragging || $0.isDecelerating
        } ?? false
        // A plain mid-resource swipe on a settled reader: the web view's
        // native paging owns it, and second-guessing a working turn is how
        // pages double. Everything else gets verified.
        guard eligible || outerEngaged || outerWasBusy else { return }
        armVerify(direction)
    }

    /// Waits for the reader to come fully to rest, then turns the page if
    /// the recognized swipe demonstrably moved nothing: the same web view
    /// is still on screen, the outer page sits where it was at touch-down,
    /// and the inner offset shows no movement beyond the settle the touch
    /// itself interrupted (see the drift rule inside). Anything more — an
    /// inner column turn, the outer sliding to another resource — means
    /// the gesture worked natively, and the verification ends without
    /// acting.
    private func armVerify(_ direction: UISwipeGestureRecognizer.Direction) {
        verifyTask?.cancel()
        let startWebView = touchDownWebView
        let innerStart = touchDownInnerOffset
        let outerPage = touchDownOuterPage
        verifyTask = Task { [weak self] in
            let deadline = ContinuousClock.now.advanced(by: .seconds(1.5))
            while ContinuousClock.now < deadline {
                try? await Task.sleep(for: .milliseconds(50))
                guard !Task.isCancelled, let self, let navigator = self.navigator else { return }
                guard let outer = self.outerScrollView else { return }
                // The navigator drops the pagination view's interaction
                // while any programmatic move or load is in flight; resting
                // scroll views mean nothing until it is idle again.
                guard outer.superview?.isUserInteractionEnabled != false else { continue }
                // The recognizing touch may still be down, and a *new*
                // touch cancels this task from touchedDown — so any
                // activity here is this gesture's own; wait it out.
                if outer.isTracking || outer.isDragging || outer.isDecelerating { continue }
                let width = outer.bounds.width
                guard width > 0 else { return }
                let page = outer.contentOffset.x / width
                // Off page alignment means a slide is still running.
                guard abs(page - page.rounded()) * width < 1 else { continue }

                // Live view, not the capture: the navigator keeps several
                // preloaded spreads around, and their model geometry can
                // advance ahead of the screen.
                guard let webView = self.visibleWebView(in: navigator.view) else { return }
                // A different spread on screen means the resource turned
                // — the gesture worked natively.
                guard webView === startWebView else { return }
                let inner = webView.scrollView
                if inner.isTracking || inner.isDragging || inner.isDecelerating { continue }

                // The outer moving to another page means the resource
                // turned natively — the gesture worked.
                guard abs(page.rounded() - outerPage) < 0.5 else { return }
                let pageWidth = inner.bounds.width
                guard pageWidth > 0 else { return }
                let drift = abs(inner.contentOffset.x - innerStart)
                let remaining = direction == .left
                    ? inner.contentSize.width - pageWidth - inner.contentOffset.x
                    : inner.contentOffset.x
                // How much inner drift still reads as a dead swipe depends
                // on where the resource rests now. Clamped in the swipe's
                // direction, the defining dead shape is a touch landing
                // while the previous turn was still settling *into* this
                // outermost page — that settle finishes under the finger,
                // so up to a page of drift is the prior turn's, not this
                // swipe's. A full page or more can only mean the swipe
                // itself turned natively (a mid-snap touch on a page that
                // was not really outermost); acting would double it.
                // Anywhere else the inner had room to move, so any drift
                // at all means the turn happened natively.
                let deadSwipe = remaining < 2
                    ? drift < pageWidth - 2
                    : drift < 1
                if deadSwipe {
                    self.turn(direction, webView: webView)
                }
                return
            }
        }
    }

    /// Drives the verified missed turn. Within the resource this is a
    /// DOM-relative instant scroll on the verified web view — immune to
    /// the navigator's lagging spread bookkeeping, so it cannot land
    /// backward or double. Only a true boundary goes through the
    /// navigator, whose index is committed now that everything rests.
    private func turn(_ direction: UISwipeGestureRecognizer.Direction, webView: WKWebView) {
        let inner = webView.scrollView
        let pageWidth = inner.bounds.width
        guard pageWidth > 0 else { return }
        let delta = direction == .left ? pageWidth : -pageWidth
        let target = inner.contentOffset.x + delta
        if target >= -2, target <= inner.contentSize.width - pageWidth + 2 {
            webView.evaluateJavaScript(
                "window.scrollBy({ left: \(delta), behavior: 'instant' });",
                completionHandler: nil
            )
            return
        }
        guard let navigator else { return }
        // Unanimated deliberately: a smooth slide here is a window for the
        // next fast swipe to interrupt it and hand the landing to the
        // pager's snap — the jump-back this class exists to prevent.
        verifyTask = Task {
            if direction == .left {
                _ = await navigator.goRight(options: NavigatorGoOptions(animated: false))
            } else {
                _ = await navigator.goLeft(options: NavigatorGoOptions(animated: false))
            }
        }
    }

    /// The spread web view currently on screen: the one covering the
    /// navigator's center and actually visible — the navigator keeps
    /// preloaded spreads at alpha 0 until they are revealed.
    private func visibleWebView(in root: UIView) -> WKWebView? {
        let center = CGPoint(x: root.bounds.midX, y: root.bounds.midY)
        var queue: [UIView] = [root]
        while let view = queue.popLast() {
            if let webView = view as? WKWebView,
               !webView.isHidden,
               webView.scrollView.alpha > 0,
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
