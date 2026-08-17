// The UniFFI-generated bindings (Generated/InkunaCore.swift) are compiled
// directly into this target, so core types need no import.
import Foundation

// MARK: - Failure reasons

/// Why one picked file did not become a book.
///
/// The core distinguishes its failure classes by `InkunaError` variant and
/// the shell routes its messaging off the variant, never off a string —
/// the batch path carries the same typed error the single-file path throws.
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
    /// Bigger than the core's import ceiling, so it was refused mid-copy
    /// instead of being allowed to fill the device.
    case tooLarge
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

    /// Maps a core error — thrown by the single-file path or carried by a
    /// batch `.failed` item, which since the FFI de-flattening are the very
    /// same `InkunaError`. The structured payload survives the boundary,
    /// so the detected format arrives as a field, never parsed out of a
    /// Display string.
    init(_ error: InkunaError) {
        switch error {
        case .UnsupportedFormat(let format): self = .unsupportedFormat(format)
        case .Archive: self = .damagedArchive
        case .InvalidPublication: self = .invalidPublication
        case .FileTooLarge: self = .tooLarge
        case .Io: self = .storage
        case .Database: self = .database
        case .InvalidPositionRanges: self = .libraryUnavailable
        case .NotFound: self = .notFound
        // The search index is derived data and no part of import; if one of
        // its failures ever surfaces here it is a bug worth reporting with
        // the core's own words rather than a wrong explanation.
        case .Search(let detail): self = .unknown(detail)
        }
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

/// Where a run has got to. Reported per file: the core's batch listener
/// fires as each file finishes, and the shell adds its own staging phases
/// around it.
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
