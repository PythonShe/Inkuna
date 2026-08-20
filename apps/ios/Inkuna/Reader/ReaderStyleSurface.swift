import WebKit

/// What the user-stylesheet pipeline needs from whatever renders the book:
/// the loaded documents to push CSS into. Deliberately separate from
/// `ReaderPagerSurface` — both are implemented by the Readium surface
/// today, and both die with it when the custom renderer lands, but the
/// styling contract is the smaller of the two and worth keeping narrow.
@MainActor
protocol ReaderStyleSurface: AnyObject {
    /// Every loaded spread document — visible and preloaded alike.
    func loadedWebViews() -> [WKWebView]
    /// The spread covering the reader's center, if any.
    func visibleWebView() -> WKWebView?
}
