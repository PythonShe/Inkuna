#if DEBUG
import Foundation

/// Scriptable cross-shell layout-digest harness. The manifest carries every
/// layout value so this runner cannot accidentally become a second settings
/// source of truth.
enum ParityDigestRunner {
    @MainActor
    static func run() async {
        guard let documents = FileManager.default.urls(
            for: .documentDirectory,
            in: .userDomainMask
        ).first else {
            print("PARITY ERROR Documents directory unavailable")
            return
        }

        let corpus = documents.appendingPathComponent("ParityCorpus", isDirectory: true)
        let manifestURL = corpus.appendingPathComponent("manifest.json")
        do {
            let manifest = try JSONDecoder().decode(
                [ParityManifestCase].self,
                from: Data(contentsOf: manifestURL)
            )
            for item in manifest {
                try item.validate()
            }
            var output = ParityDigestOutput()
            for item in manifest {
                output.append(
                    file: URL(fileURLWithPath: item.file).lastPathComponent,
                    value: await digestWithTimeout(item, corpus: corpus)
                )
            }
            try output.encodedData().write(
                to: documents.appendingPathComponent("parity-ios.json"),
                options: .atomic
            )
            print("PARITY DONE \(manifest.count) books")
        } catch {
            print("PARITY ERROR \(errorMessage(error))")
        }
    }

    private static func digestWithTimeout(
        _ item: ParityManifestCase,
        corpus: URL
    ) async -> ParityBookValue {
        let gate = ParityResultGate()
        let work = Task {
            await gate.resolve(await digest(item, corpus: corpus))
        }
        let timeout = Task {
            try? await Task.sleep(for: .seconds(120))
            await gate.resolve(.string("TIMEOUT"))
        }
        let value = await gate.wait()
        work.cancel()
        timeout.cancel()
        return value
    }

    private static func digest(_ item: ParityManifestCase, corpus: URL) async -> ParityBookValue {
        let bookURL = corpus.appendingPathComponent(item.file)
        guard FileManager.default.fileExists(atPath: bookURL.path) else {
            return .string("ERROR: File not found: \(item.file)")
        }

        do {
            let shelf = try await LibraryStore.shared.library()
            let imported = try await shelf.importer().import(path: bookURL.path)
            let publicationID: String
            switch imported {
            case let .imported(publication), let .duplicate(publication):
                publicationID = publication.id
            case let .failed(_, error):
                return .string("ERROR: \(errorMessage(error))")
            }

            let completion = ParityCompletion()
            let listener = ParityLayoutListener(completion: completion)
            let session = try await shelf.openReader(
                id: publicationID,
                viewport: item.viewport.coreValue,
                settings: item.settings.coreValue,
                listener: listener
            )
            let spineCount = session.spineCount()
            // `chapter` is cache-only: its expected NotReady result queues every
            // spine through the engine's own worker before we await terminals.
            for spineIdx in 0..<spineCount {
                _ = try? session.chapter(spineIdx: spineIdx)
            }
            if let failedSpine = try await completion.wait(for: spineCount) {
                return .string("ERROR: chapter \(failedSpine) failed")
            }

            var chapters: [ParityChapterDigest] = []
            for spineIdx in 0..<spineCount {
                while true {
                    let terminalEvents = await completion.terminalEventCount(for: spineIdx)
                    do {
                        let chapter = try session.chapter(spineIdx: spineIdx)
                        var pages: [String] = []
                        for pageIdx in 0..<chapter.pageCount {
                            pages.append(try session.pageDigest(spineIdx: spineIdx, pageIdx: pageIdx))
                        }
                        chapters.append(ParityChapterDigest(spineIdx: spineIdx, digests: pages))
                        break
                    } catch let error as InkunaError {
                        guard case .NotReady = error else {
                            return .string("ERROR: \(errorMessage(error))")
                        }
                        if try await completion.waitForTerminalEvent(spineIdx, after: terminalEvents) {
                            return .string("ERROR: chapter \(spineIdx) failed")
                        }
                    } catch {
                        return .string("ERROR: \(errorMessage(error))")
                    }
                }
            }
            return .pages(chapters)
        } catch {
            return .string("ERROR: \(errorMessage(error))")
        }
    }

    private static func errorMessage(_ error: Error) -> String {
        guard let error = error as? InkunaError else {
            return error.localizedDescription
        }
        switch error {
        case let .Io(detail), let .Database(detail), let .Archive(detail),
             let .InvalidPublication(detail), let .NotReady(detail),
             let .UnsupportedContent(detail), let .LayoutBudgetExceeded(detail),
             let .AnchorNotFound(detail), let .Search(detail):
            return detail
        case let .FileTooLarge(limit):
            return "limit=\(limit)"
        case let .UnsupportedFormat(format):
            return "format=\(format ?? "nil")"
        case let .NotFound(id):
            return "id=\(id)"
        }
    }
}

private struct ParityManifestCase: Decodable, Sendable {
    let file: String
    let viewport: ParityManifestViewport
    let settings: ParityManifestSettings

    func validate() throws {
        guard viewport.width.isFinite, viewport.width > 0, viewport.height.isFinite, viewport.height > 0 else {
            throw ParityManifestError.invalidViewport(file)
        }
    }
}

private enum ParityManifestError: LocalizedError {
    case invalidViewport(String)

    var errorDescription: String? {
        switch self {
        case let .invalidViewport(file):
            return "Invalid viewport for \(file)"
        }
    }
}

private struct ParityManifestViewport: Decodable, Sendable {
    let width: Double
    let height: Double

    var coreValue: Viewport {
        Viewport(width: width, height: height)
    }
}

private struct ParityManifestSettings: Decodable, Sendable {
    let readingFont: String
    let readingBold: Bool
    let textSizeStep: UInt8
    let lineSpacing: Double
    let letterSpacing: Double
    let wordSpacing: Double
    let readingMargins: UInt32

    private enum CodingKeys: String, CodingKey {
        case readingFont = "reading_font"
        case readingBold = "reading_bold"
        case textSizeStep = "text_size_step"
        case lineSpacing = "line_spacing"
        case letterSpacing = "letter_spacing"
        case wordSpacing = "word_spacing"
        case readingMargins = "reading_margins"
    }

    var coreValue: ReaderLayoutSettings {
        ReaderLayoutSettings(
            readingFont: readingFont,
            readingBold: readingBold,
            textSizeStep: textSizeStep,
            lineSpacing: lineSpacing,
            letterSpacing: letterSpacing,
            wordSpacing: wordSpacing,
            readingMargins: readingMargins
        )
    }
}

private struct ParityChapterDigest: Sendable {
    let spineIdx: UInt32
    let digests: [String]
}

private enum ParityBookValue: Sendable {
    case pages([ParityChapterDigest])
    case string(String)
}

private struct ParityDigestOutput {
    private var books: [(String, ParityBookValue)] = []

    mutating func append(file: String, value: ParityBookValue) {
        books.append((file, value))
    }

    func encodedData() throws -> Data {
        let entries = try books.map { file, value in
            "\(try jsonString(file)):\(try jsonValue(value))"
        }
        return Data("{\(entries.joined(separator: ","))}".utf8)
    }

    private func jsonValue(_ value: ParityBookValue) throws -> String {
        switch value {
        case let .string(message):
            return try jsonString(message)
        case let .pages(chapters):
            let entries = try chapters.map { chapter in
                "\(try jsonString(String(chapter.spineIdx))):\(try jsonArray(chapter.digests))"
            }
            return "{\(entries.joined(separator: ","))}"
        }
    }

    private func jsonArray(_ values: [String]) throws -> String {
        "[\(try values.map(jsonString).joined(separator: ","))]"
    }

    private func jsonString(_ value: String) throws -> String {
        String(decoding: try JSONEncoder().encode(value), as: UTF8.self)
    }
}

private actor ParityResultGate {
    private var value: ParityBookValue?
    private var waiter: CheckedContinuation<ParityBookValue, Never>?

    func resolve(_ newValue: ParityBookValue) {
        guard value == nil else { return }
        value = newValue
        waiter?.resume(returning: newValue)
        waiter = nil
    }

    func wait() async -> ParityBookValue {
        if let value { return value }
        return await withCheckedContinuation { waiter = $0 }
    }
}

private actor ParityCompletion {
    private var terminalSpines = Set<UInt32>()
    private var terminalEventCounts: [UInt32: Int] = [:]
    private var terminalEventFailures: [UInt32: Bool] = [:]
    private var terminalEventWaiters: [UInt32: CheckedContinuation<Bool, Error>] = [:]
    private var failedSpine: UInt32?
    private var expectedSpines: UInt32?
    private var waiter: CheckedContinuation<UInt32?, Error>?

    func chapterReady(_ spineIdx: UInt32) {
        record(spineIdx, failed: false)
    }

    func chapterFailed(_ spineIdx: UInt32) {
        record(spineIdx, failed: true)
    }

    func wait(for spineCount: UInt32) async throws -> UInt32? {
        expectedSpines = spineCount
        if terminalSpines.count >= Int(spineCount) { return failedSpine }
        return try await withTaskCancellationHandler(operation: {
            try await withCheckedThrowingContinuation { continuation in
                if Task.isCancelled {
                    continuation.resume(throwing: CancellationError())
                } else {
                    waiter = continuation
                }
            }
        }, onCancel: {
            Task { await self.cancelInitialWait() }
        })
    }

    func terminalEventCount(for spineIdx: UInt32) -> Int {
        terminalEventCounts[spineIdx, default: 0]
    }

    func waitForTerminalEvent(_ spineIdx: UInt32, after count: Int) async throws -> Bool {
        if terminalEventCounts[spineIdx, default: 0] > count {
            return terminalEventFailures[spineIdx, default: false]
        }
        return try await withTaskCancellationHandler(operation: {
            try await withCheckedThrowingContinuation { continuation in
                if Task.isCancelled {
                    continuation.resume(throwing: CancellationError())
                } else {
                    terminalEventWaiters[spineIdx] = continuation
                }
            }
        }, onCancel: {
            Task { await self.cancelTerminalEventWait(for: spineIdx) }
        })
    }

    private func cancelInitialWait() {
        guard let waiter else { return }
        self.waiter = nil
        waiter.resume(throwing: CancellationError())
    }

    private func cancelTerminalEventWait(for spineIdx: UInt32) {
        guard let waiter = terminalEventWaiters.removeValue(forKey: spineIdx) else { return }
        waiter.resume(throwing: CancellationError())
    }

    private func record(_ spineIdx: UInt32, failed: Bool) {
        terminalEventCounts[spineIdx, default: 0] += 1
        terminalEventFailures[spineIdx] = failed
        let eventWaiter = terminalEventWaiters.removeValue(forKey: spineIdx)
        eventWaiter?.resume(returning: failed)
        guard terminalSpines.insert(spineIdx).inserted else { return }
        if failed { failedSpine = min(failedSpine ?? spineIdx, spineIdx) }
        guard
            let expectedSpines,
            terminalSpines.count >= Int(expectedSpines),
            let waiter
        else { return }
        self.waiter = nil
        waiter.resume(returning: failedSpine)
    }
}

private final class ParityLayoutListener: LayoutListener, @unchecked Sendable {
    private let completion: ParityCompletion

    init(completion: ParityCompletion) {
        self.completion = completion
    }

    func onFirstPageReady(generation _: UInt64, spineIdx _: UInt32) {}

    func onChapterReady(generation _: UInt64, spineIdx: UInt32, pageCount _: UInt32) {
        Task { await completion.chapterReady(spineIdx) }
    }

    func onChapterFailed(generation _: UInt64, spineIdx: UInt32) {
        Task { await completion.chapterFailed(spineIdx) }
    }
}
#endif
