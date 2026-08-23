import UIKit

enum ChapterReadiness {
    case empty
    case partial(publishedPages: UInt32)
    case complete(geometry: ChapterGeometry)
}

/// Engine-backed pager state. All FFI reads here are synchronous cache reads.
@MainActor
final class EnginePagerSurface: ReaderPagerSurface {
    private let session: ReaderSession
    private let canvas: EnginePageCanvas
    private var readiness: [UInt32: ChapterReadiness] = [:]
    private var failedSpines: Set<UInt32> = []
    private var latestGeneration: UInt64?
    private var pendingGeneration = false
    private var innerOffset: CGFloat = 0
    private var outerDisplacement: CGFloat = 0
    private var lastSettledPage: UInt32?

    private(set) var spineIdx: UInt32 = 0
    private(set) var pageIdx: UInt32 = 0
    var spineCount: UInt32 = 0
    var onPageSettled: ((UInt32, UInt32) -> Void)?
    var selectionActive = false

    init(session: ReaderSession, canvas: EnginePageCanvas) {
        self.session = session
        self.canvas = canvas
    }

    var isEngageable: Bool {
        guard !isBusy, canvas.isLaidOut, !failedSpines.contains(spineIdx) else { return false }
        guard let list = try? session.page(spineIdx: spineIdx, pageIdx: pageIdx) else { return false }
        return latestGeneration == nil || list.generation == latestGeneration
    }

    var isBusy: Bool { pendingGeneration }
    var hasActiveSelection: Bool { selectionActive }
    var isRightToLeft: Bool { session.isRtl() }

    func display(spineIdx: UInt32, pageIdx: UInt32) {
        self.spineIdx = spineIdx
        self.pageIdx = pageIdx
        innerOffset = CGFloat(pageIdx) * pageWidth
        outerDisplacement = 0
        lastSettledPage = pageIdx
        canvas.showUnreadablePlaceholder(failedSpines.contains(spineIdx))
        setScene()
        onPageSettled?(spineIdx, pageIdx)
    }

    func firstPageBecameReady(generation: UInt64, spineIdx: UInt32) {
        guard accept(generation: generation) else { return }
        let published = max(1, session.publishedPageCount(spineIdx: spineIdx))
        if case let .complete(geometry)? = readiness[spineIdx], geometry.generation == generation {
            // Completion may arrive before this hop reaches the main actor.
        } else {
            readiness[spineIdx] = .partial(publishedPages: published)
        }
        if spineIdx == self.spineIdx {
            pendingGeneration = false
            canvas.showUnreadablePlaceholder(false)
            setScene()
        }
    }

    func chapterBecameReady(generation: UInt64, spineIdx: UInt32) {
        guard accept(generation: generation), let geometry = try? session.chapter(spineIdx: spineIdx),
              geometry.generation == generation else { return }
        readiness[spineIdx] = .complete(geometry: geometry)
        if spineIdx == self.spineIdx {
            pendingGeneration = false
            canvas.showUnreadablePlaceholder(false)
            setScene()
        }
    }

    func chapterFailed(generation: UInt64, spineIdx: UInt32) {
        guard accept(generation: generation) else { return }
        readiness[spineIdx] = .empty
        failedSpines.insert(spineIdx)
        if spineIdx == self.spineIdx {
            pendingGeneration = false
            canvas.showUnreadablePlaceholder(true)
            setScene()
        }
    }

    /// `updateLayout` returns `Void` in the generated bindings. The caller
    /// therefore passes 0 until the next engine callback supplies the real
    /// generation; no shell counter is created.
    func layoutInvalidated(generation: UInt64) {
        latestGeneration = generation == 0 ? nil : generation
        pendingGeneration = true
        readiness.removeAll()
        failedSpines.removeAll()
        lastSettledPage = nil
        canvas.invalidateAll()
    }

    func innerMetrics() -> ReaderPagerStrip? {
        guard pageWidth > 0 else { return nil }
        switch readiness[spineIdx] ?? .empty {
        case let .complete(geometry):
            if let latestGeneration, geometry.generation != latestGeneration { return nil }
            let end = CGFloat(max(1, geometry.pageCount) - 1) * pageWidth
            return ReaderPagerStrip(offset: innerOffset, range: 0 ... end, pageWidth: pageWidth)
        case let .partial(publishedPages):
            let published = max(publishedPages, session.publishedPageCount(spineIdx: spineIdx))
            let end = CGFloat(max(1, published) - 1) * pageWidth
            return ReaderPagerStrip(
                offset: innerOffset,
                range: 0 ... max(innerOffset, end),
                pageWidth: pageWidth
            )
        case .empty:
            return nil
        }
    }

    func setInnerOffset(_ x: CGFloat) {
        guard let metrics = innerMetrics() else { return }
        innerOffset = min(max(x, metrics.range.lowerBound), metrics.range.upperBound)
        setScene()
        let settled = UInt32((innerOffset / metrics.pageWidth).rounded())
        if abs(innerOffset - CGFloat(settled) * metrics.pageWidth) < 0.01,
           settled != lastSettledPage {
            pageIdx = settled
            lastSettledPage = settled
            onPageSettled?(spineIdx, settled)
        }
    }

    func outerMetrics() -> ReaderPagerStrip? {
        guard pageWidth > 0 else { return nil }
        let leftExists = neighborSpine(toRight: false) != nil
        let rightExists = neighborSpine(toRight: true) != nil
        return ReaderPagerStrip(
            offset: pageWidth,
            range: (leftExists ? 0 : pageWidth) ... (rightExists ? 2 * pageWidth : pageWidth),
            pageWidth: pageWidth
        )
    }

    func setOuterOffset(_ x: CGFloat) {
        guard let metrics = outerMetrics() else { return }
        let offset = min(max(x, metrics.range.lowerBound), metrics.range.upperBound)
        outerDisplacement = pageWidth - offset
        setScene()
    }

    func neighborIsReady(toRight: Bool) -> Bool {
        guard let neighbor = neighborSpine(toRight: toRight) else { return false }
        if isForward(toRight: toRight) {
            switch readiness[neighbor] {
            case .partial?, .complete?:
                return pageIsCached(spineIdx: neighbor, pageIdx: 0)
            case .empty, nil:
                _ = try? session.page(spineIdx: neighbor, pageIdx: 0)
                return false
            }
        }
        guard case let .complete(geometry)? = readiness[neighbor], geometry.pageCount > 0 else {
            _ = try? session.chapter(spineIdx: neighbor)
            return false
        }
        return pageIsCached(spineIdx: neighbor, pageIdx: geometry.pageCount - 1)
    }

    func commitBoundaryCrossing(toRight: Bool) -> Bool {
        guard let target = neighborSpine(toRight: toRight) else { return false }
        if isForward(toRight: toRight) {
            guard neighborIsReady(toRight: toRight) else { return false }
            spineIdx = target
            pageIdx = 0
            innerOffset = 0
        } else {
            guard case let .complete(geometry)? = readiness[target], geometry.pageCount > 0,
                  pageIsCached(spineIdx: target, pageIdx: geometry.pageCount - 1) else { return false }
            spineIdx = target
            pageIdx = geometry.pageCount - 1
            innerOffset = CGFloat(pageIdx) * pageWidth
        }
        outerDisplacement = 0
        lastSettledPage = pageIdx
        canvas.showUnreadablePlaceholder(false)
        setScene()
        onPageSettled?(spineIdx, pageIdx)
        return true
    }

    func pagePoint(fromCanvasPoint point: CGPoint) -> CGPoint? {
        canvas.pagePoint(spineIdx: spineIdx, pageIdx: pageIdx, from: point)
    }

    private var pageWidth: CGFloat { canvas.bounds.width }

    private func accept(generation: UInt64) -> Bool {
        if let latestGeneration, generation != latestGeneration { return false }
        if latestGeneration == nil {
            latestGeneration = generation
            canvas.invalidate(generation: generation)
        }
        return true
    }

    private func setScene() {
        let count: UInt32
        switch readiness[spineIdx] ?? .empty {
        case let .complete(geometry): count = geometry.pageCount
        case let .partial(publishedPages): count = max(publishedPages, session.publishedPageCount(spineIdx: spineIdx))
        case .empty: count = 0
        }
        let edge: (spineIdx: UInt32, pageIdx: UInt32, toRight: Bool)?
        if outerDisplacement == 0 {
            edge = nil
        } else {
            let toRight = outerDisplacement < 0
            edge = neighborEntry(toRight: toRight)
        }
        canvas.setScene(PageScene(
            spineIdx: spineIdx,
            pageCount: count,
            rtl: isRightToLeft,
            innerOffset: innerOffset,
            outerDisplacement: outerDisplacement,
            neighborEdge: edge
        ))
    }

    private func neighborSpine(toRight: Bool) -> UInt32? {
        let delta = toRight ? (isRightToLeft ? -1 : 1) : (isRightToLeft ? 1 : -1)
        let candidate = Int64(spineIdx) + Int64(delta)
        guard candidate >= 0, candidate < Int64(spineCount) else { return nil }
        return UInt32(candidate)
    }

    private func isForward(toRight: Bool) -> Bool { toRight != isRightToLeft }

    private func neighborEntry(toRight: Bool) -> (spineIdx: UInt32, pageIdx: UInt32, toRight: Bool)? {
        guard let neighbor = neighborSpine(toRight: toRight), neighborIsReady(toRight: toRight) else { return nil }
        if isForward(toRight: toRight) {
            return (neighbor, 0, toRight)
        }
        guard case let .complete(geometry)? = readiness[neighbor], geometry.pageCount > 0 else { return nil }
        return (neighbor, geometry.pageCount - 1, toRight)
    }

    private func pageIsCached(spineIdx: UInt32, pageIdx: UInt32) -> Bool {
        guard let list = try? session.page(spineIdx: spineIdx, pageIdx: pageIdx) else { return false }
        return latestGeneration == nil || list.generation == latestGeneration
    }
}
