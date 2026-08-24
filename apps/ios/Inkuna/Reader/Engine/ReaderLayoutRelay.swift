import Foundation

/// Bridges engine-thread layout notifications onto the UIKit main actor.
final class ReaderLayoutRelay: LayoutListener, @unchecked Sendable {
    private let firstPageReady: @Sendable (UInt64, UInt32) -> Void
    private let chapterReady: @Sendable (UInt64, UInt32, UInt32) -> Void
    private let chapterFailed: @Sendable (UInt64, UInt32) -> Void

    init(
        onFirstPageReady: @escaping @Sendable (UInt64, UInt32) -> Void,
        onChapterReady: @escaping @Sendable (UInt64, UInt32, UInt32) -> Void,
        onChapterFailed: @escaping @Sendable (UInt64, UInt32) -> Void
    ) {
        firstPageReady = onFirstPageReady
        chapterReady = onChapterReady
        chapterFailed = onChapterFailed
    }

    func onFirstPageReady(generation: UInt64, spineIdx: UInt32) {
        let callback = firstPageReady
        Task { @MainActor in callback(generation, spineIdx) }
    }

    func onChapterReady(generation: UInt64, spineIdx: UInt32, pageCount: UInt32) {
        let callback = chapterReady
        Task { @MainActor in callback(generation, spineIdx, pageCount) }
    }

    func onChapterFailed(generation: UInt64, spineIdx: UInt32) {
        let callback = chapterFailed
        Task { @MainActor in callback(generation, spineIdx) }
    }
}
