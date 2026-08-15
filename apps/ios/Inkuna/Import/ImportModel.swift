// The UniFFI-generated bindings (Generated/InkunaCore.swift) are compiled
// directly into this target, so core types need no import.
import Foundation

// MARK: - Failure reasons

/// Why one picked file did not become a book.
///
/// The core distinguishes its failure classes by `InkunaError` variant and
/// the shell routes its messaging off the variant, never off a string —
/// with one unavoidable exception noted in ``init(coreMessage:)``.
enum ImportFailureReason: Error, Sendable, Equatable {
    /// The file is a book Inkuna does not read yet. Carries the format the
    /// core detected from the file's magic bytes (`"mobi"`, `"cbz"`, …), or
    /// `nil` when nothing recognizable was found — the core names the
    /// format precisely so we can promise the format rather than apologize.
    case unsupportedFormat(String?)
    /// The zip container is damaged; the file is not readable as an archive.
    case damagedArchive
    /// A well-formed EPUB whose structure the core cannot import.
    case invalidPublication
    /// Filesystem trouble: unreadable source, no room, a device error.
    case storage
    /// The library database itself failed the write.
    case database
    /// The core reported nothing by that id (should not reach import).
    case notFound
    /// Shell-side: the file could not be copied out of its provider —
    /// a denied sandbox extension, a cloud file that would not download.
    case unreadableSource
    /// Shell-side: the library would not open at all, so nothing could be
    /// imported. The user needs the library screen's recovery path, not a
    /// per-file explanation.
    case libraryUnavailable
    /// A failure class this build does not know. Carries the core's own
    /// message so a bug report is still actionable.
    case unknown(String)

    /// Maps a thrown core error. `InkunaError` is a UniFFI *flat* error, so
    /// each case carries the whole `Display` string in `message` rather
    /// than the structured payload the Rust enum holds; the variant is
    /// still the thing we switch on.
    init(_ error: InkunaError) {
        switch error {
        case .UnsupportedFormat(let message):
            self = .unsupportedFormat(Self.formatName(fromUnsupported: message))
        case .Archive: self = .damagedArchive
        case .InvalidPublication: self = .invalidPublication
        case .Io: self = .storage
        case .Database: self = .database
        case .NotFound: self = .notFound
        }
    }

    /// Maps a batch item's failure message.
    ///
    /// `ImportOutcome.failed` carries only `error.to_string()` — the typed
    /// variant is lost at the FFI boundary on the batch path (unlike the
    /// single-file path, which throws a typed `InkunaError`). Recovering
    /// the class therefore means matching the `thiserror` `Display`
    /// prefixes declared in `inkuna-ffi`, which are stable strings in the
    /// core's source. An unmatched message degrades to ``unknown(_:)``
    /// carrying the original text, never to a wrong promise.
    init(coreMessage message: String) {
        switch true {
        case message.hasPrefix("unsupported format"):
            self = .unsupportedFormat(Self.formatName(fromUnsupported: message))
        case message.hasPrefix("archive error"): self = .damagedArchive
        case message.hasPrefix("invalid publication"): self = .invalidPublication
        case message.hasPrefix("io error"): self = .storage
        case message.hasPrefix("database error"): self = .database
        case message.hasPrefix("publication not found"): self = .notFound
        default: self = .unknown(message)
        }
    }

    /// Pulls `cbz` out of `unsupported format: cbz`. The core omits the
    /// suffix entirely when it recognized nothing, which is the `nil` case.
    private static func formatName(fromUnsupported message: String?) -> String? {
        guard let message, let separator = message.range(of: ": ") else { return nil }
        let name = message[separator.upperBound...].trimmingCharacters(in: .whitespacesAndNewlines)
        return name.isEmpty ? nil : name
    }
}

/// One file that failed, named so the user knows which one.
struct ImportFailure: Sendable, Equatable {
    /// The file's own name as the user sees it in Files, not a temp path.
    let fileName: String
    let reason: ImportFailureReason
}

// MARK: - Outcomes

/// One requested file's result, after both the shell's staging step and
/// the core's pipeline have had their say.
enum ImportItemOutcome: Sendable {
    case imported(Publication, fileName: String)
    /// The library already holds this content; nothing was added.
    case duplicate(Publication, fileName: String)
    case failed(ImportFailure)

    var fileName: String {
        switch self {
        case .imported(_, let fileName), .duplicate(_, let fileName): fileName
        case .failed(let failure): failure.fileName
        }
    }
}

/// The result of one import run, in the order the user picked the files.
struct ImportReport: Sendable {
    let items: [ImportItemOutcome]
    /// True when the user cancelled part-way; files already imported stay
    /// imported, because the core commits per file.
    let wasCancelled: Bool

    init(items: [ImportItemOutcome] = [], wasCancelled: Bool = false) {
        self.items = items
        self.wasCancelled = wasCancelled
    }

    var imported: [Publication] {
        items.compactMap { if case .imported(let publication, _) = $0 { publication } else { nil } }
    }

    var duplicates: [Publication] {
        items.compactMap { if case .duplicate(let publication, _) = $0 { publication } else { nil } }
    }

    var failures: [ImportFailure] {
        items.compactMap { if case .failed(let failure) = $0 { failure } else { nil } }
    }

    var isEmpty: Bool { items.isEmpty }

    /// True when the library gained something and screens should reload.
    var didChangeLibrary: Bool { !imported.isEmpty }

    /// A run that added everything it was given, with nothing to explain.
    var isCleanSuccess: Bool {
        !items.isEmpty && duplicates.isEmpty && failures.isEmpty && !wasCancelled
    }

    /// Anything the user would want itemized — a duplicate they need named,
    /// a file that did not make it — earns the summary sheet rather than a
    /// toast that swallows the detail.
    var needsSummary: Bool {
        items.count > 1 && !isCleanSuccess
    }
}

// MARK: - Progress

/// Where a run has got to. Reported per chunk, not per byte: the core's
/// import is one call per file with no inner progress to observe.
struct ImportProgress: Sendable {
    enum Phase: Sendable, Equatable {
        /// Copying the picked file out of its provider into staging.
        case preparing
        /// The core is hashing, parsing, and committing.
        case importing
    }

    var completed: Int
    var total: Int
    /// The file being worked on, for the status line.
    var fileName: String?
    var phase: Phase

    var fraction: Double {
        total > 0 ? Double(completed) / Double(total) : 0
    }
}
