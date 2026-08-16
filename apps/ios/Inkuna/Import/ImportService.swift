import Foundation

/// Drives the core's import pipeline for a selection of files.
///
/// Main-actor isolated on purpose. Every step here is an `await` that
/// suspends — staging runs on a detached task, and the core's methods do
/// their work on Rust threads — so the main thread is never blocked, while
/// the caller gets progress callbacks that arrive in order, on the actor
/// that draws them, with no `Sendable` gymnastics in between.
@MainActor
enum ImportService {
    /// Files handed to the core per call.
    ///
    /// The core's `importBatch` parallelizes the parse stage with rayon, so
    /// bigger chunks import faster, and its progress listener reports every
    /// finished file while the call runs. Chunking survives for disk, not
    /// feedback: at most four staged copies exist at a time, not the whole
    /// selection.
    private static let chunkSize = 4

    /// Imports `urls` in order, reporting progress as it goes.
    ///
    /// Never throws: a run that fails wholesale comes back as a report in
    /// which every item failed, because the caller has to tell the user
    /// which files did not make it either way.
    ///
    /// - Parameter onProgress: called on the main actor; escaping because
    ///   the core's per-file events arrive while `importBatch` is awaited.
    static func run(
        urls: [URL],
        onProgress: @escaping @MainActor (ImportProgress) -> Void = { _ in }
    ) async -> ImportReport {
        guard !urls.isEmpty else { return ImportReport() }

        let shelf: Bookshelf
        do {
            shelf = try await LibraryStore.shared.library()
        } catch {
            return ImportReport(items: urls.map {
                .failed(ImportFailure(fileName: $0.importDisplayName, reason: .libraryUnavailable))
            })
        }

        // Safe here and only here: no other run can be in flight, because
        // `ImportFlow` serializes them.
        await detached { ImportStaging.sweep() }

        var items: [ImportItemOutcome] = []
        var completed = 0
        var cancelled = false

        for chunk in urls.chunked(into: chunkSize) {
            if Task.isCancelled {
                cancelled = true
                break
            }

            onProgress(ImportProgress(
                completed: completed,
                total: urls.count,
                fileName: chunk.first?.importDisplayName,
                phase: .preparing
            ))
            let staged = await detached { chunk.map(StagedFile.init(source:)) }

            onProgress(ImportProgress(
                completed: completed,
                total: urls.count,
                fileName: chunk.first?.importDisplayName,
                phase: .importing
            ))
            items.append(contentsOf: await importStaged(
                staged,
                using: shelf,
                completedBefore: completed,
                totalCount: urls.count,
                onProgress: onProgress
            ))

            let readable = staged.compactMap(\.staged)
            await detached { readable.forEach(ImportStaging.discard) }

            completed += chunk.count
        }

        onProgress(ImportProgress(
            completed: completed,
            total: urls.count,
            fileName: nil,
            phase: .importing
        ))
        return ImportReport(items: items, wasCancelled: cancelled)
    }

    // MARK: Core calls

    /// Hands one chunk's staged copies to the core and maps every outcome
    /// back onto the file the user actually picked. Progress within the
    /// chunk comes from the core's listener, one event per finished file.
    private static func importStaged(
        _ staged: [StagedFile],
        using shelf: Bookshelf,
        completedBefore: Int,
        totalCount: Int,
        onProgress: @escaping @MainActor (ImportProgress) -> Void
    ) async -> [ImportItemOutcome] {
        let readable = staged.filter { $0.staged != nil }
        guard !readable.isEmpty else {
            return staged.map { .failed(ImportFailure(fileName: $0.fileName, reason: $0.failureReason)) }
        }

        let outcomes: [Result<ImportOutcome, ImportFailureReason>]
        if readable.count == 1, let only = readable.first, let path = only.staged?.path(percentEncoded: false) {
            // One file goes through `import`, which reports failure by
            // throwing rather than by a `.failed` item; either way the
            // error is the same typed `InkunaError`.
            do {
                outcomes = [.success(try await shelf.import(path: path))]
            } catch let error as InkunaError {
                outcomes = [.failure(ImportFailureReason(error))]
            } catch {
                outcomes = [.failure(.unknown(error.localizedDescription))]
            }
        } else {
            let paths = readable.compactMap { $0.staged?.path(percentEncoded: false) }
            // The listener names files by our staging path; walk each event
            // back to the name the user picked for the status line.
            let names = Dictionary(
                readable.compactMap { file in
                    file.staged.map { ($0.path(percentEncoded: false), file.fileName) }
                },
                uniquingKeysWith: { first, _ in first }
            )
            let listener = ChunkProgressListener { completedInChunk, path in
                let progress = ImportProgress(
                    completed: completedBefore + completedInChunk,
                    total: totalCount,
                    fileName: names[path],
                    phase: .importing
                )
                // The main queue is FIFO, so events keep the core's order
                // and the count never appears to move backwards.
                DispatchQueue.main.async {
                    MainActor.assumeIsolated { onProgress(progress) }
                }
            }
            do {
                // Per-item failures come back as `.failed` items in input
                // order; only a wholesale failure throws.
                outcomes = try await shelf.importBatch(paths: paths, listener: listener).map { .success($0) }
            } catch let error as InkunaError {
                let reason = ImportFailureReason(error)
                outcomes = readable.map { _ in .failure(reason) }
            } catch {
                let reason = ImportFailureReason.unknown(error.localizedDescription)
                outcomes = readable.map { _ in .failure(reason) }
            }
        }

        // Walk the core's answers back onto the readable files in order —
        // the batch contract is one outcome per input path, in input order
        // — slotting the files that never reached the core back into the
        // positions the user picked them in.
        var answers = outcomes.makeIterator()
        return staged.map { file in
            guard file.staged != nil else {
                return .failed(ImportFailure(fileName: file.fileName, reason: file.failureReason))
            }
            guard let outcome = answers.next() else {
                // Cannot happen while the core honors input order and
                // arity; reported honestly rather than silently dropped.
                return .failed(ImportFailure(
                    fileName: file.fileName,
                    reason: .unknown("the core returned no outcome for this file")
                ))
            }
            return item(for: outcome, file: file)
        }
    }

    private static func item(
        for outcome: Result<ImportOutcome, ImportFailureReason>,
        file: StagedFile
    ) -> ImportItemOutcome {
        switch outcome {
        case .success(.imported(let publication)):
            .imported(publication, fileName: file.fileName)
        case .success(.duplicate(let publication)):
            .duplicate(publication, fileName: file.fileName)
        case .success(.failed(_, let error)):
            // The path in a `.failed` item is our staging path, which means
            // nothing to the user — name the file they picked instead.
            .failed(ImportFailure(fileName: file.fileName, reason: ImportFailureReason(error)))
        case .failure(let reason):
            .failed(ImportFailure(fileName: file.fileName, reason: reason))
        }
    }

    /// Runs blocking filesystem work off the main thread.
    private static func detached<T: Sendable>(_ work: @escaping @Sendable () -> T) async -> T {
        await Task.detached(priority: .userInitiated, operation: work).value
    }
}

// MARK: - Progress listener

/// Forwards the core's per-file completion events for one `importBatch`
/// call. The core calls from its worker threads; the stored closure is
/// `@Sendable` and does its own main-queue hop.
private final class ChunkProgressListener: ImportProgressListener, @unchecked Sendable {
    private let deliver: @Sendable (_ completedInChunk: Int, _ path: String) -> Void

    init(deliver: @escaping @Sendable (Int, String) -> Void) {
        self.deliver = deliver
    }

    func onFileComplete(completed: UInt32, total: UInt32, path: String) {
        deliver(Int(completed), path)
    }
}

// MARK: - Staged file

/// A picked file paired with the private copy the core will read, or
/// `nil` when it could not be copied out of its provider at all.
private struct StagedFile: Sendable {
    let fileName: String
    let staged: URL?
    /// Why staging failed; meaningful only while `staged` is nil.
    let failureReason: ImportFailureReason

    init(source: URL) {
        fileName = source.importDisplayName
        do {
            staged = try ImportStaging.stage(source)
            failureReason = .unreadableSource
        } catch is ImportStaging.FileTooLarge {
            staged = nil
            failureReason = .tooLarge
        } catch {
            staged = nil
            failureReason = .unreadableSource
        }
    }
}

// MARK: - Chunking

extension Array {
    /// Splits into consecutive slices of at most `size`, preserving order.
    func chunked(into size: Int) -> [[Element]] {
        guard size > 0 else { return isEmpty ? [] : [self] }
        return stride(from: 0, to: count, by: size).map {
            Array(self[$0 ..< Swift.min($0 + size, count)])
        }
    }
}
