import UIKit

enum ChapterReadiness {
    case empty
    case partial(publishedPages: UInt32)
    case complete(geometry: ChapterGeometry)
}

/// The sole logical-page ↔ physical-slot conversion. The engine always
/// numbers pages in reading order; only the shell mirrors their strip slot
/// for RTL progression.
enum PageSlot {
    static func slot(for pageIdx: UInt32, pageCount: UInt32, rtl: Bool) -> UInt32 {
        guard pageIdx < pageCount else { return 0 }
        return rtl ? pageCount - pageIdx - 1 : pageIdx
    }

    static func pageIdx(for slot: UInt32, pageCount: UInt32, rtl: Bool) -> UInt32 {
        guard slot < pageCount else { return 0 }
        return rtl ? pageCount - slot - 1 : slot
    }

    static func offset(
        forPageIdx pageIdx: UInt32,
        pageCount: UInt32,
        rtl: Bool,
        pageWidth: CGFloat
    ) -> CGFloat {
        CGFloat(slot(for: pageIdx, pageCount: pageCount, rtl: rtl)) * pageWidth
    }
}

/// Engine-backed pager state. All FFI reads here are synchronous cache reads.
@MainActor
final class EnginePagerSurface: ReaderPagerSurface {
    private let session: ReaderSession
    private let canvas: EnginePageCanvas
    private var readiness: [UInt32: ChapterReadiness] = [:]
    private var failedSpines: Set<UInt32> = []
    private var neighborReadiness: [NeighborKey: Bool] = [:]
    private var latestGeneration: UInt64?
    private var pendingGeneration = false
    private var innerOffset: CGFloat = 0
    private var outerDisplacement: CGFloat = 0
    private var scenePageCount: UInt32 = 0
    private var interactionPageCount: UInt32?
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
        guard !isBusy, canvas.isLaidOut else { return false }
        if failedSpines.contains(spineIdx) { return true }
        guard let latestGeneration else { return false }
        guard let list = try? session.page(spineIdx: spineIdx, pageIdx: pageIdx) else { return false }
        return list.generation == latestGeneration
    }

    var isBusy: Bool { pendingGeneration }
    var hasActiveSelection: Bool { selectionActive }
    var isRightToLeft: Bool { session.isRtl() }

    func display(spineIdx: UInt32, pageIdx: UInt32) {
        self.spineIdx = spineIdx
        self.pageIdx = pageIdx
        interactionPageCount = nil
        primeReadiness(for: spineIdx)
        let count = pageCount(for: spineIdx)
        innerOffset = PageSlot.offset(
            forPageIdx: pageIdx,
            pageCount: count,
            rtl: isRightToLeft,
            pageWidth: pageWidth
        )
        outerDisplacement = 0
        scenePageCount = count
        lastSettledPage = pageIdx
        neighborReadiness.removeAll()
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
        failedSpines.remove(spineIdx)
        neighborReadiness.removeAll()
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
        failedSpines.remove(spineIdx)
        neighborReadiness.removeAll()
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
        neighborReadiness.removeAll()
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
        neighborReadiness.removeAll()
        scenePageCount = 0
        interactionPageCount = nil
        lastSettledPage = nil
        canvas.invalidateAll()
    }

    func beginPagingInteraction() {
        let count = pageCount(for: spineIdx)
        rebaseInnerOffset(for: count)
        interactionPageCount = count
    }

    func endPagingInteraction() {
        interactionPageCount = nil
        setScene()
    }

    func innerMetrics() -> ReaderPagerStrip? {
        guard pageWidth > 0 else { return nil }
        let count = displayedPageCount()
        rebaseInnerOffset(for: count)
        // The strip's extent comes from the displayed count, so an
        // interaction's frozen count also freezes the range it can reach.
        let end = CGFloat(max(1, count) - 1) * pageWidth
        switch readiness[spineIdx] ?? .empty {
        case let .complete(geometry):
            if let latestGeneration, geometry.generation != latestGeneration { return nil }
            return ReaderPagerStrip(offset: innerOffset, range: 0 ... end, pageWidth: pageWidth)
        case .partial:
            return ReaderPagerStrip(
                offset: innerOffset,
                range: 0 ... max(innerOffset, end),
                pageWidth: pageWidth
            )
        case .empty:
            if failedSpines.contains(spineIdx) {
                return ReaderPagerStrip(offset: innerOffset, range: 0 ... 0, pageWidth: pageWidth)
            }
            return nil
        }
    }

    func setInnerOffset(_ x: CGFloat) {
        guard let metrics = innerMetrics() else { return }
        innerOffset = min(max(x, metrics.range.lowerBound), metrics.range.upperBound)
        setScene()
        let slot = UInt32((innerOffset / metrics.pageWidth).rounded())
        let settled = PageSlot.pageIdx(
            for: slot,
            pageCount: displayedPageCount(),
            rtl: isRightToLeft
        )
        if abs(innerOffset - PageSlot.offset(
            forPageIdx: settled,
            pageCount: displayedPageCount(),
            rtl: isRightToLeft,
            pageWidth: metrics.pageWidth
        )) < 0.01,
           settled != lastSettledPage {
            pageIdx = settled
            lastSettledPage = settled
            onPageSettled?(spineIdx, settled)
        }
    }

    /// The live outer offset — the exact inverse of `setOuterOffset`, so a
    /// pager that re-reads it mid-flight sees the boundary displacement
    /// currently on screen instead of the committed home.
    func outerMetrics() -> ReaderPagerStrip? {
        guard pageWidth > 0 else { return nil }
        let leftExists = neighborSpine(toRight: false) != nil
        let rightExists = neighborSpine(toRight: true) != nil
        return ReaderPagerStrip(
            offset: pageWidth - outerDisplacement,
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
        let key = NeighborKey(spineIdx: neighbor, toRight: toRight)
        if let ready = neighborReadiness[key] { return ready }
        let ready: Bool
        if failedSpines.contains(neighbor) {
            // A failed chapter occupies one placeholder page; it must stay
            // crossable in both directions or every chapter beyond it
            // becomes unreachable by paging.
            ready = true
        } else if isForward(toRight: toRight) {
            ready = session.publishedPageCount(spineIdx: neighbor) > 0
            if !ready {
                _ = try? session.chapter(spineIdx: neighbor)
            }
        } else {
            ready = completeGeometry(neighbor) != nil
        }
        neighborReadiness[key] = ready
        return ready
    }

    func commitBoundaryCrossing(toRight: Bool) -> Bool {
        guard let target = neighborSpine(toRight: toRight) else { return false }
        guard neighborIsReady(toRight: toRight) else { return false }
        let targetPage: UInt32
        if failedSpines.contains(target) || isForward(toRight: toRight) {
            targetPage = 0
        } else {
            guard let geometry = completeGeometry(target) else { return false }
            targetPage = geometry.pageCount - 1
        }
        spineIdx = target
        pageIdx = targetPage
        let count = pageCount(for: target)
        innerOffset = PageSlot.offset(
            forPageIdx: targetPage,
            pageCount: count,
            rtl: isRightToLeft,
            pageWidth: pageWidth
        )
        scenePageCount = count
        interactionPageCount = count
        outerDisplacement = 0
        lastSettledPage = pageIdx
        neighborReadiness.removeAll()
        canvas.showUnreadablePlaceholder(failedSpines.contains(target))
        setScene()
        onPageSettled?(spineIdx, pageIdx)
        return true
    }

    func pagePoint(fromCanvasPoint point: CGPoint) -> CGPoint? {
        canvas.pagePoint(spineIdx: spineIdx, pageIdx: pageIdx, from: point)
    }

    private var pageWidth: CGFloat { canvas.bounds.width }

    private struct NeighborKey: Hashable {
        let spineIdx: UInt32
        let toRight: Bool
    }

    private func pageCount(for spineIdx: UInt32) -> UInt32 {
        switch readiness[spineIdx] ?? .empty {
        case let .complete(geometry): geometry.pageCount
        case let .partial(publishedPages): max(publishedPages, session.publishedPageCount(spineIdx: spineIdx))
        case .empty: failedSpines.contains(spineIdx) ? 1 : 0
        }
    }

    /// Learns a spine's published prefix when a display gets ahead of its
    /// main-actor layout callback (for example after a recreated surface).
    private func primeReadiness(for spineIdx: UInt32) {
        guard readiness[spineIdx] == nil, !failedSpines.contains(spineIdx) else { return }
        let published = session.publishedPageCount(spineIdx: spineIdx)
        if published > 0 { readiness[spineIdx] = .partial(publishedPages: published) }
    }

    /// Complete geometry with at least one page, healing the readiness map
    /// from the session cache (a recreated surface starts with an empty map
    /// even when the session has already laid the chapter out). A miss
    /// schedules the chapter and answers nil.
    private func completeGeometry(_ spineIdx: UInt32) -> ChapterGeometry? {
        if case let .complete(geometry)? = readiness[spineIdx], geometry.pageCount > 0 {
            return geometry
        }
        guard let geometry = try? session.chapter(spineIdx: spineIdx) else { return nil }
        if let latestGeneration, geometry.generation != latestGeneration { return nil }
        guard geometry.pageCount > 0 else { return nil }
        readiness[spineIdx] = .complete(geometry: geometry)
        return geometry
    }

    private func accept(generation: UInt64) -> Bool {
        if let latestGeneration, generation != latestGeneration { return false }
        if latestGeneration == nil {
            latestGeneration = generation
            canvas.invalidate(generation: generation)
        }
        return true
    }

    private func setScene() {
        let count = displayedPageCount()
        rebaseInnerOffset(for: count)
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

    private func rebaseInnerOffset(for pageCount: UInt32) {
        defer { scenePageCount = pageCount }
        guard isRightToLeft, scenePageCount > 0, pageCount > scenePageCount else { return }
        innerOffset += CGFloat(pageCount - scenePageCount) * pageWidth
    }

    /// The strip's page count. During a paging interaction it is frozen so
    /// layout events cannot remap slots under the finger — except that in
    /// LTR progression newly published pages append past the strip's end
    /// without moving any existing offset, so growth is adopted live and a
    /// drag can reach pages published during it. In RTL, growth would
    /// rebase every offset; it stays deferred to `endPagingInteraction()`.
    private func displayedPageCount() -> UInt32 {
        guard let frozen = interactionPageCount else { return pageCount(for: spineIdx) }
        guard !isRightToLeft else { return frozen }
        let live = pageCount(for: spineIdx)
        guard live > frozen else { return frozen }
        interactionPageCount = live
        return live
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
        if failedSpines.contains(neighbor) { return (neighbor, 0, toRight) }
        if isForward(toRight: toRight) {
            return (neighbor, 0, toRight)
        }
        guard let geometry = completeGeometry(neighbor) else { return nil }
        return (neighbor, geometry.pageCount - 1, toRight)
    }

}
