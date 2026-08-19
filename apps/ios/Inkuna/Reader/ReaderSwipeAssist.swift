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
/// display-link verify that waits for every scroll view to rest and then checks,
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
///   at-rest checks so the spread index it consults is committed.
///
/// Both turns animate like native ones: what made animation dangerous
/// before was firing into an in-flight scroll, and firing now only ever
/// happens from a verified idle, at-rest, page-aligned reader — a smooth
/// rescue already running is caught by the alignment gate, not composed
/// with.
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

    /// The verify runs on a display link, not a timer: the gates it polls
    /// (deceleration flags, page alignment, the navigator's interaction
    /// tell) all flip inside UIKit's per-frame scroll servicing, so a
    /// frame callback lands immediately after they change, while a sleep
    /// loop lands at an arbitrary phase up to a full frame late — and
    /// follows the panel to 120 Hz instead of assuming 60. The link
    /// retains its target while running; every exit path goes through
    /// `stopVerify()`, which breaks that cycle no later than the deadline.
    private var verifyLink: CADisplayLink?
    private var verifyDeadline = ContinuousClock.now
    private var verifyDirection: UISwipeGestureRecognizer.Direction = .left

    /// Baselines the armed verify compares against. The start web view is
    /// held strongly so a deallocation can never masquerade as the
    /// "no baseline" case (nil means the touch landed on a loading
    /// reader).
    private var verifyStartWebView: WKWebView?
    private var verifyInnerStart: CGFloat = 0
    private var verifyOuterPage: CGFloat = 0

    /// The boundary turn lives in its own slot: a fresh touch cancels the
    /// verify, but must not cancel a turn already handed to the navigator —
    /// cancellation would propagate into Readium's transition and cut short
    /// the anti-flash sleep inside its unanimated slide. The navigator's
    /// own state machine coalesces whatever the next gesture asks for.
    private var turnTask: Task<Void, Never>?

    /// Fired on any recognized horizontal swipe — native or about to be
    /// rescued — so the reader can clear its chrome the moment turn intent
    /// shows instead of when the turn lands.
    var onPageTurnGesture: (() -> Void)?

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
        // No verifyLink handling: a running link retains self, so deinit
        // can only run once the verify has already stopped.
        turnTask?.cancel()
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
        stopVerify()
        eligibleLeft = false
        eligibleRight = false
        outerWasBusy = false
        touchDownWebView = nil
        outerScrollView = nil
        guard
            let navigator,
            // Scroll mode has no page snaps to rescue. Paginated
            // vertical-writing publications still page along x (the axis
            // is only vertical for scrolled non-vertical text), so they
            // pass this guard and are handled like any other.
            navigator.presentation.axis == .horizontal,
            !navigator.presentation.scroll,
            // Any spread will do for finding the pagination scroll view —
            // including the alpha-0 ones a loading reader is left with.
            let anySpread = anyWebView(in: navigator.view)
        else { return }
        let outer = nearestScrollView(above: anySpread)
        outerScrollView = outer
        if let outer, outer.bounds.width > 0 {
            let page = outer.contentOffset.x / outer.bounds.width
            touchDownOuterPage = page.rounded()
            // Decelerating, frozen off alignment by this very touch, or
            // locked out by a programmatic slide or load: this touch is
            // landing on a still-settling turn.
            outerWasBusy = outer.isDecelerating ||
                abs(page - page.rounded()) * outer.bounds.width > 1 ||
                outer.superview?.isUserInteractionEnabled == false
        }

        // No revealed spread — the centered one is still loading, so this
        // touch is guaranteed dead (the navigator has input disabled).
        // Leave the baseline nil; the verify adopts whatever is on screen
        // once the reader rests.
        guard let webView = visibleWebView(in: navigator.view) else { return }
        touchDownWebView = webView

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
        onPageTurnGesture?()
        let direction = recognizer.direction
        let eligible = direction == .left ? eligibleLeft : eligibleRight
        let outerEngaged = outerScrollView.map {
            $0.isTracking || $0.isDragging || $0.isDecelerating
        } ?? false
        // A plain mid-resource swipe on a settled reader: the web view's
        // native paging owns it, and second-guessing a working turn is how
        // pages double. Everything else gets verified.
        guard eligible || outerEngaged || outerWasBusy else { return }
        if fireNowIfProvablyDead(direction) { return }
        armVerify(direction)
    }

    /// The clean dead fling, resolved with zero latency: when nothing
    /// anywhere is in flight and the resource rests clamped in the swipe's
    /// direction, nothing native can honor the gesture — the checks that
    /// the verify loop would wait to confirm all pass right now, so the
    /// turn fires immediately and the boundary feels like any other page.
    /// Any condition unmet falls through to the verify loop instead.
    private func fireNowIfProvablyDead(_ direction: UISwipeGestureRecognizer.Direction) -> Bool {
        guard
            let navigator,
            let outer = outerScrollView,
            // Idle navigator; an outer neither owned by this touch nor
            // settling, resting on a page.
            outer.superview?.isUserInteractionEnabled != false,
            !outer.isTracking, !outer.isDragging, !outer.isDecelerating,
            outer.bounds.width > 0
        else { return false }
        let outerPage = outer.contentOffset.x / outer.bounds.width
        guard abs(outerPage - outerPage.rounded()) * outer.bounds.width < 1 else { return false }
        guard let webView = visibleWebView(in: navigator.view) else { return false }
        // The finger may still be down (tracking) — that is fine on a
        // clamped edge; a deceleration or an off-page offset is not: a
        // turn is in flight and could be about to honor this swipe.
        let inner = webView.scrollView
        guard !inner.isDecelerating else { return false }
        let pageWidth = inner.bounds.width
        guard pageWidth > 0 else { return false }
        let innerPage = inner.contentOffset.x / pageWidth
        guard abs(innerPage - innerPage.rounded()) * pageWidth < 1 else { return false }
        let remaining = direction == .left
            ? inner.contentSize.width - pageWidth - inner.contentOffset.x
            : inner.contentOffset.x
        guard remaining < 2 else { return false }
        turn(direction, webView: webView)
        return true
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
        stopVerify()
        verifyDirection = direction
        verifyStartWebView = touchDownWebView
        verifyInnerStart = touchDownInnerOffset
        verifyOuterPage = touchDownOuterPage
        verifyDeadline = ContinuousClock.now.advanced(by: .seconds(1.5))
        let link = CADisplayLink(target: self, selector: #selector(verifyTick))
        // Ride the panel's full rate (needs CADisableMinimumFrameDuration-
        // OnPhone, set in project.yml): the rescue lands on the very frame
        // the settle it is waiting out ends. The low minimum lets the
        // system relax the panel while nothing is changing; on 60 Hz
        // hardware the range simply clamps.
        link.preferredFrameRateRange = CAFrameRateRange(minimum: 30, maximum: 120, preferred: 120)
        // .common, not .default: the link must keep firing while the
        // recognizing touch is still tracking a scroll view.
        link.add(to: .main, forMode: .common)
        verifyLink = link
    }

    private func stopVerify() {
        verifyLink?.invalidate()
        verifyLink = nil
        verifyStartWebView = nil
    }

    /// One frame of the verify: the old poll-loop body. A plain `return`
    /// is "still settling, look again next frame"; `stopVerify()` before
    /// a return is a verdict — the gesture either worked natively or has
    /// just been rescued.
    @objc private func verifyTick(_ link: CADisplayLink) {
        guard link === verifyLink else {
            // A superseded link that somehow ticks once more must not
            // outlive its replacement.
            link.invalidate()
            return
        }
        guard ContinuousClock.now < verifyDeadline, let navigator, let outer = outerScrollView else {
            stopVerify()
            return
        }
        // The navigator drops the pagination view's interaction while any
        // programmatic move or load is in flight; resting scroll views
        // mean nothing until it is idle again.
        guard outer.superview?.isUserInteractionEnabled != false else { return }
        // The recognizing touch may still be down, and a *new* touch stops
        // this verify from touchedDown — so any activity here is this
        // gesture's own; wait it out.
        if outer.isTracking || outer.isDragging || outer.isDecelerating { return }
        let width = outer.bounds.width
        guard width > 0 else {
            stopVerify()
            return
        }
        let page = outer.contentOffset.x / width
        // Off page alignment means a slide is still running.
        guard abs(page - page.rounded()) * width < 1 else { return }

        // Live view, not the capture: the navigator keeps several
        // preloaded spreads around, and their model geometry can advance
        // ahead of the screen.
        guard let webView = visibleWebView(in: navigator.view) else {
            stopVerify()
            return
        }
        // A different spread on screen means the resource turned — the
        // gesture worked natively. No baseline at all means the touch
        // landed on a loading reader whose input was disabled: nothing
        // could have moved natively, so the swipe is dead by construction
        // and the comparisons below have nothing to compare against.
        if let startWebView = verifyStartWebView, webView !== startWebView {
            stopVerify()
            return
        }
        let inner = webView.scrollView
        if inner.isTracking || inner.isDragging || inner.isDecelerating { return }

        // The outer moving to another page means the resource turned
        // natively — the gesture worked.
        if verifyStartWebView != nil, abs(page.rounded() - verifyOuterPage) >= 0.5 {
            stopVerify()
            return
        }
        let pageWidth = inner.bounds.width
        guard pageWidth > 0 else {
            stopVerify()
            return
        }
        // A smooth JS scroll (a previous rescue's, or Readium's own
        // animated turn) moves the offset without any of the UIScrollView
        // flags above; off page alignment is its only tell. Firing into
        // one would compose the two scrolls and hand the landing to the
        // pager's snap.
        let innerPage = inner.contentOffset.x / pageWidth
        guard abs(innerPage - innerPage.rounded()) * pageWidth < 1 else { return }
        let drift = verifyStartWebView == nil
            ? 0
            : abs(inner.contentOffset.x - verifyInnerStart)
        let remaining = verifyDirection == .left
            ? inner.contentSize.width - pageWidth - inner.contentOffset.x
            : inner.contentOffset.x
        // How much inner drift still reads as a dead swipe depends on
        // where the resource rests now. Clamped in the swipe's direction,
        // the defining dead shape is a touch landing while the previous
        // turn was still settling *into* this outermost page — that
        // settle finishes under the finger, so up to a page of drift is
        // the prior turn's, not this swipe's. A full page or more can
        // only mean the swipe itself turned natively (a mid-snap touch on
        // a page that was not really outermost); acting would double it.
        // Anywhere else the inner had room to move, so any drift at all
        // means the turn happened natively.
        let deadSwipe = remaining < 2
            ? drift < pageWidth - 2
            : drift < 1
        if deadSwipe {
            turn(verifyDirection, webView: webView)
        }
        stopVerify()
    }

    /// Drives the verified missed turn, animated like a native one —
    /// firing only ever happens from a verified idle, at-rest, page-aligned
    /// reader, which is what made animation dangerous before. Within the
    /// resource this is a DOM-relative scroll on the verified web view
    /// (Readium's own turn primitive) — immune to the navigator's lagging
    /// spread bookkeeping, so it cannot land backward or double. Only a
    /// true boundary goes through the navigator, whose index is committed
    /// now that everything rests.
    private func turn(_ direction: UISwipeGestureRecognizer.Direction, webView: WKWebView) {
        let inner = webView.scrollView
        let pageWidth = inner.bounds.width
        guard pageWidth > 0 else { return }
        let delta = direction == .left ? pageWidth : -pageWidth
        let target = inner.contentOffset.x + delta
        if target >= -2, target <= inner.contentSize.width - pageWidth + 2 {
            webView.evaluateJavaScript(
                "window.scrollBy({ left: \(delta), behavior: 'smooth' });",
                completionHandler: nil
            )
            return
        }
        guard let navigator else { return }
        turnTask = Task { [weak navigator] in
            guard let navigator else { return }
            if direction == .left {
                _ = await navigator.goRight(options: NavigatorGoOptions(animated: true))
            } else {
                _ = await navigator.goLeft(options: NavigatorGoOptions(animated: true))
            }
        }
    }

    /// The last answer `visibleWebView` walked the tree for. The verify
    /// asks every frame and the walk crosses WKWebView's internal
    /// hierarchy, so a still-visible previous answer short-circuits it;
    /// visibility is re-proved live on each use, so a spread change is
    /// caught on the frame it happens.
    private weak var cachedVisibleWebView: WKWebView?

    /// The spread web view currently on screen: the one covering the
    /// navigator's center and actually visible — the navigator keeps
    /// preloaded spreads at alpha 0 until they are revealed.
    private func visibleWebView(in root: UIView) -> WKWebView? {
        if let cached = cachedVisibleWebView, isVisibleSpread(cached, in: root) {
            return cached
        }
        var queue: [UIView] = [root]
        while let view = queue.popLast() {
            if let webView = view as? WKWebView, isVisibleSpread(webView, in: root) {
                cachedVisibleWebView = webView
                return webView
            }
            queue.append(contentsOf: view.subviews)
        }
        return nil
    }

    private func isVisibleSpread(_ webView: WKWebView, in root: UIView) -> Bool {
        webView.isDescendant(of: root) &&
            !webView.isHidden &&
            webView.scrollView.alpha > 0 &&
            webView.convert(webView.bounds, to: root)
                .contains(CGPoint(x: root.bounds.midX, y: root.bounds.midY))
    }

    /// Any spread web view at all, revealed or not — only good for walking
    /// up to the pagination scroll view they all share.
    private func anyWebView(in root: UIView) -> WKWebView? {
        var queue: [UIView] = [root]
        while let view = queue.popLast() {
            if let webView = view as? WKWebView { return webView }
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
