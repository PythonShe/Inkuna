import ReadiumNavigator
import UIKit
import WebKit

/// What the pager needs from whatever renders the book: two horizontal
/// strips (the pages inside the current resource, and the resources
/// themselves), a way to silence the renderer's own gestures, and a hook
/// to commit a resource crossing the pager has already animated.
///
/// `ReaderPager` speaks only this protocol. `ReadiumPagerSurface` is the
/// one implementation today and dies with the Readium navigator when the
/// custom reader frontend lands — the pager, its physics, and its feel
/// carry over unchanged.
@MainActor
protocol ReaderPagerSurface: AnyObject {
    /// True when the reader is paginated along x and the strips resolve.
    /// False disengages the pager entirely (scroll mode, fixed layout,
    /// or a hierarchy this surface no longer recognizes — the fail-open
    /// path that leaves the renderer's native handling in charge).
    var isEngageable: Bool { get }
    /// True while the renderer runs its own move (a jump, a preference
    /// reload, a resource load). The pager never captures baselines from
    /// a busy renderer.
    var isBusy: Bool { get }
    /// True while text is selected — horizontal drags then belong to the
    /// selection handles, never to page turns.
    var hasActiveSelection: Bool { get }
    /// Whether the publication reads right-to-left. The pager itself is
    /// purely geometric; this only maps "forward" for keys and taps.
    var isRightToLeft: Bool { get }

    /// Disables the renderer's own horizontal gesture handling.
    /// Idempotent, and safe to call often — the pager re-enforces it on
    /// every navigation change because the renderer creates new views as
    /// it preloads.
    func suppressNativeGestures()
    /// Re-enables native handling when the pager disengages.
    func restoreNativeGestures()

    /// The current resource's page strip, resolved live. Nil while no
    /// resource is on screen or its geometry is degenerate.
    func innerMetrics() -> ReaderPagerStrip?
    func setInnerOffset(_ x: CGFloat)
    /// The resource strip, resolved live.
    func outerMetrics() -> ReaderPagerStrip?
    func setOuterOffset(_ x: CGFloat)

    /// Commits a resource crossing the pager has already animated to its
    /// exact landing offset: the renderer updates its own bookkeeping
    /// (index, preloads, locator) without moving anything on screen.
    func commitBoundaryCrossing(toRight: Bool) async -> Bool
}

/// One horizontal strip: where it sits, how far it goes, and its page.
struct ReaderPagerStrip {
    var offset: CGFloat
    var range: ClosedRange<CGFloat>
    var pageWidth: CGFloat
}

/// The Readium-backed surface. Everything Readium-shaped lives here — the
/// hierarchy walking, the pan-recognizer suppression, the `goLeft`/
/// `goRight` commit — so replacing the navigator later replaces exactly
/// this file.
///
/// How the takeover works (verified against the Readium 3.x sources):
///
/// - Paging is 100% native `UIScrollView` panning — pages inside a
///   resource on the spread web view's own scroll view, resources on the
///   outer pagination scroll view. Readium's JS never consumes horizontal
///   touches, so disabling those two pan recognizers is a complete
///   takeover. Readium toggles `isScrollEnabled` in several places but
///   never touches `panGestureRecognizer.isEnabled`, which is why the
///   suppression targets the recognizers, not the scroll flags.
/// - Taps, chrome, footnotes, and selection ride a separate pipeline
///   (JS pointer events → `didTapAt`, WebKit's own selection gestures)
///   and keep working untouched.
/// - The commit relies on a verified no-op: with the outer strip already
///   resting exactly on the neighbor's slot, `goRight`/`goLeft`
///   (unanimated) fails its within-resource attempt at the clamp, falls
///   back to the pagination view's own index move, and lands where the
///   screen already is — state committed, nothing visibly moves. And
///   because the outer pan is disabled, Readium's spread index can only
///   ever change inside its own `goToIndex`, so the index the commit
///   consults is never stale — the lag that used to turn a boundary
///   gesture into a backward jump is structurally gone.
@MainActor
final class ReadiumPagerSurface: ReaderPagerSurface {
    private weak var navigator: EPUBNavigatorViewController?

    init(navigator: EPUBNavigatorViewController) {
        self.navigator = navigator
    }

    // MARK: Engagement

    var isEngageable: Bool {
        guard let navigator else { return false }
        return navigator.presentation.axis == .horizontal &&
            !navigator.presentation.scroll &&
            outerScrollView() != nil
    }

    /// The navigator's own tell: it drops the pagination view's
    /// interaction during every programmatic move and load.
    var isBusy: Bool {
        outerScrollView()?.superview?.isUserInteractionEnabled == false
    }

    var hasActiveSelection: Bool {
        navigator?.currentSelection != nil
    }

    var isRightToLeft: Bool {
        navigator?.presentation.readingProgression == .rtl
    }

    // MARK: Gesture suppression

    func suppressNativeGestures() {
        setNativeGestures(enabled: false)
    }

    func restoreNativeGestures() {
        setNativeGestures(enabled: true)
    }

    private func setNativeGestures(enabled: Bool) {
        guard let navigator else { return }
        outerScrollView()?.panGestureRecognizer.isEnabled = enabled
        for webView in allWebViews(in: navigator.view) {
            webView.scrollView.panGestureRecognizer.isEnabled = enabled
        }
    }

    // MARK: Strips

    func innerMetrics() -> ReaderPagerStrip? {
        guard let inner = visibleInnerScrollView() else { return nil }
        let width = inner.bounds.width
        guard width > 0 else { return nil }
        return ReaderPagerStrip(
            offset: inner.contentOffset.x,
            range: 0 ... max(0, inner.contentSize.width - width),
            pageWidth: width
        )
    }

    func setInnerOffset(_ x: CGFloat) {
        guard let inner = visibleInnerScrollView() else { return }
        inner.contentOffset = CGPoint(x: x, y: inner.contentOffset.y)
    }

    func outerMetrics() -> ReaderPagerStrip? {
        guard let outer = outerScrollView() else { return nil }
        let width = outer.bounds.width
        guard width > 0 else { return nil }
        return ReaderPagerStrip(
            offset: outer.contentOffset.x,
            range: 0 ... max(0, outer.contentSize.width - width),
            pageWidth: width
        )
    }

    func setOuterOffset(_ x: CGFloat) {
        guard let outer = outerScrollView() else { return }
        outer.contentOffset = CGPoint(x: x, y: outer.contentOffset.y)
    }

    // MARK: Commit

    func commitBoundaryCrossing(toRight: Bool) async -> Bool {
        guard let navigator else { return false }
        let options = NavigatorGoOptions(animated: false)
        return toRight
            ? await navigator.goRight(options: options)
            : await navigator.goLeft(options: options)
    }

    // MARK: Hierarchy

    /// The pagination scroll view, re-proved live on each use: the cached
    /// answer is only trusted while it is still in a window.
    private weak var cachedOuterScrollView: UIScrollView?

    private func outerScrollView() -> UIScrollView? {
        if let cached = cachedOuterScrollView, cached.window != nil {
            return cached
        }
        guard let navigator, let anySpread = anyWebView(in: navigator.view) else { return nil }
        let outer = nearestScrollView(above: anySpread)
        cachedOuterScrollView = outer
        return outer
    }

    /// The last answer `visibleInnerScrollView` walked the tree for; the
    /// walk crosses WKWebView's internal hierarchy, so a still-visible
    /// previous answer short-circuits it. Visibility is re-proved live on
    /// each use, so a spread change is caught the frame it happens.
    private weak var cachedVisibleWebView: WKWebView?

    /// The spread web view currently covering the reader's center — the
    /// navigator keeps preloaded spreads at alpha 0 until revealed.
    private func visibleInnerScrollView() -> UIScrollView? {
        guard let navigator else { return nil }
        let root: UIView = navigator.view
        if let cached = cachedVisibleWebView, isVisibleSpread(cached, in: root) {
            return cached.scrollView
        }
        var queue: [UIView] = [root]
        while let view = queue.popLast() {
            if let webView = view as? WKWebView, isVisibleSpread(webView, in: root) {
                cachedVisibleWebView = webView
                return webView.scrollView
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

    private func allWebViews(in root: UIView) -> [WKWebView] {
        var found: [WKWebView] = []
        var queue: [UIView] = [root]
        while let view = queue.popLast() {
            if let webView = view as? WKWebView {
                found.append(webView)
                // A web view's own subtree holds no further spreads.
                continue
            }
            queue.append(contentsOf: view.subviews)
        }
        return found
    }

    /// The pagination scroll view: the nearest scroll view ancestor of a
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
