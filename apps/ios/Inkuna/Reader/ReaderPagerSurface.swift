import UIKit

/// The engine-shaped strip contract shared by the native reader pagers.
@MainActor
protocol ReaderPagerSurface: AnyObject {
    var isEngageable: Bool { get }
    var isBusy: Bool { get }
    var hasActiveSelection: Bool { get }
    var isRightToLeft: Bool { get }
    func innerMetrics() -> ReaderPagerStrip?
    func setInnerOffset(_ x: CGFloat)
    func outerMetrics() -> ReaderPagerStrip?
    func setOuterOffset(_ x: CGFloat)
    func neighborIsReady(toRight: Bool) -> Bool
    func commitBoundaryCrossing(toRight: Bool) -> Bool
}

struct ReaderPagerStrip {
    var offset: CGFloat
    var range: ClosedRange<CGFloat>
    var pageWidth: CGFloat
}
