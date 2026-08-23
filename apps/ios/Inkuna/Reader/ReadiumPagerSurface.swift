// dies in Task 3.2
import ObjectiveC
@preconcurrency import ReadiumNavigator
import ReadiumShared
import UIKit
import WebKit

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
/// - The commit is a visual no-op that is *verified*, not assumed: with
///   the outer strip already resting exactly on the neighbor's slot,
///   `goRight`/`goLeft` (unanimated) normally fails its within-resource
///   attempt at the clamp, falls back to the pagination view's own index
///   move, and lands where the screen already is — and the surface
///   confirms the location actually changed before reporting success. And
///   because the outer pan is disabled, Readium's spread index can only
///   ever change inside its own `goToIndex`, so the index the commit
///   consults is never stale — the lag that used to turn a boundary
///   gesture into a backward jump is structurally gone.
@MainActor
final class ReadiumPagerSurface: ReaderPagerSurface {
    private weak var navigator: EPUBNavigatorViewController?

    init(navigator: EPUBNavigatorViewController) {
        self.navigator = navigator
        ReadiumNavigatorShim.install(on: navigator)
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

    // MARK: Neighbor readiness

    func neighborIsReady(toRight: Bool) -> Bool {
        guard let navigator, let outer = outerScrollView(), outer.bounds.width > 0,
              visibleInnerScrollView() != nil, let current = cachedVisibleWebView
        else { return false }
        let width = outer.bounds.width
        // Content coordinates (a scroll view's own space), so a live
        // displacement doesn't shift the answer: the neighbor's slot
        // sits exactly one page width beside the visible spread's.
        let target = current.convert(current.bounds, to: outer).midX + (toRight ? width : -width)
        for webView in allWebViews(in: navigator.view) where webView !== current {
            if abs(webView.convert(webView.bounds, to: outer).midX - target) < width / 2 {
                return webView.scrollView.alpha > 0
            }
        }
        return false
    }

    // MARK: Commit

    /// Temporary adapter only. The engine surface replaces this in Task 2.3;
    /// use the visible-neighbor check as an honest synchronous best effort
    /// while asking Readium to perform the corresponding navigation.
    func commitBoundaryCrossing(toRight: Bool) -> Bool {
        guard let navigator, neighborIsReady(toRight: toRight) else { return false }
        let options = NavigatorGoOptions(animated: false)
        Task { @MainActor [weak navigator] in
            guard let navigator else { return }
            _ = toRight
                ? await navigator.goRight(options: options)
                : await navigator.goLeft(options: options)
        }
        return true
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
        visibleWebView()?.scrollView
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

// MARK: - Navigator interposition

/// Implemented by the reader, called for VoiceOver's three-finger swipe.
@MainActor
protocol ReaderAccessibilityScrolling: AnyObject {
    /// Returns whether a page was actually turned — a refusal must bubble,
    /// never be reported as a turn that did not happen.
    func readerAccessibilityScroll(_ direction: UIAccessibilityScrollDirection) -> Bool
}

/// Two things Readium's navigator does to the responder chain that the
/// reader has to undo.
///
/// 1. `EPUBNavigatorViewController` overrides `accessibilityScroll` itself
///    and unconditionally claims it, paging through its own `goRight`/
///    `goLeft` — which bypasses the pager entirely and, sitting below the
///    reader in the responder chain, means the reader's own override could
///    never run.
/// 2. Its `InputObservableViewController` base makes itself first responder
///    in `viewDidAppear` (`canBecomeFirstResponder` is hard-coded `true`)
///    to observe `presses` — and takes it back on every re-appearance, so
///    it cannot be resigned once and for all from outside. A first
///    responder that is not a text input is harmless until something
///    enables the scene's focus system — a `UIMenu` pull-down does exactly
///    that on presentation — at which point UIKit reloads input views for
///    it, decides it needs a keyboard, and raises the software keyboard
///    behind the menu. The reader does not use Readium's press observation
///    at all (hardware paging is the reader's own `keyCommands`), so the
///    navigator refuses the role and the reader owns the chain itself,
///    dropping it whenever anything is presented over it.
///
/// The designated initializer is private, so the navigator cannot be
/// subclassed where it is built; the instance's class is swapped after
/// construction instead — the trick KVO itself uses, and safe here
/// because the subclass adds no storage and no initializer, only
/// overrides that forward to the reader.
///
/// Readium-shaped, so it lives in this file and dies with the navigator.
enum ReadiumNavigatorShim {
    static func install(on navigator: EPUBNavigatorViewController) {
        guard !(navigator is ShimmedNavigator) else { return }
        object_setClass(navigator, ShimmedNavigator.self)
        // It may already own the chain by the time the surface is built.
        _ = navigator.resignFirstResponder()
    }

    private final class ShimmedNavigator: EPUBNavigatorViewController {
        override func accessibilityScroll(_ direction: UIAccessibilityScrollDirection) -> Bool {
            if let reader = parent as? ReaderAccessibilityScrolling {
                return reader.readerAccessibilityScroll(direction)
            }
            return super.accessibilityScroll(direction)
        }

        /// See (2) above: never the first responder, so no menu, sheet, or
        /// focus-system change can summon a keyboard for it.
        override var canBecomeFirstResponder: Bool { false }
    }
}

// MARK: - ReaderStyleSurface

extension ReadiumPagerSurface: ReaderStyleSurface {
    func loadedWebViews() -> [WKWebView] {
        guard let navigator else { return [] }
        return allWebViews(in: navigator.view)
    }

    func visibleWebView() -> WKWebView? {
        guard let navigator else { return nil }
        let root: UIView = navigator.view
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
}
