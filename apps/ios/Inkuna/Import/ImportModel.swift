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
        // Neither of these is a fault in the file: the library itself
        // never opened — a database written by a newer build, or a
        // migration that refused its own precondition. The open-failure
        // path already meets the user with that exact sentence, so these
        // arms — which only catch an error that somehow reaches import
        // anyway — say it too rather than invent a second one. Their
        // payloads stay out of the user's way on purpose: the schema
        // numbers are versions of the database, not of the app, and a
        // migration's `detail` is for logs. Neither is a sentence anyone
        // can act on, and `.unknown` would put one on screen verbatim,
        // untranslated, in every one of our languages.
        case .SchemaTooNew, .MigrationPrecondition: self = .libraryUnavailable
        case .NotFound: self = .notFound
        // The search index is derived data and no part of import — as the
        // reader engine's own errors are no part of it either; if one of
        // their failures ever surfaces here it is a bug worth reporting
        // with the core's own words rather than a wrong explanation.
        case .Search(let detail): self = .unknown(detail)
        case .NotReady(let detail): self = .unknown(detail)
        case .UnsupportedContent(let detail): self = .unknown(detail)
        case .LayoutBudgetExceeded(let detail): self = .unknown(detail)
        case .AnchorNotFound(let detail): self = .unknown(detail)
        case .InvalidState(let detail): self = .unknown(detail)
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

/// A book whose content matched one the reader had removed.
///
/// Removal is a tombstone, not an erasure: the file and cover go, the
/// reading history stays. Importing the same content again attaches that
/// history back to it — position, bookmarks, sessions, finished state — so
/// this is an addition the reader is owed a different sentence about than
/// a plain import.
struct RestoredBook: Sendable {
    let publication: Publication
    /// False when the exact position and bookmark coordinates could not be
    /// carried over, because the core's canonical text projection changed
    /// while the book was away. The book still reopens at its remembered
    /// progress, just not on the very page — which the report says plainly
    /// rather than promising a place it cannot keep.
    let coordinatesRestored: Bool
}

/// One requested file's result, after both the shell's staging step and
/// the core's pipeline have had their say.
enum ImportItemOutcome: Sendable {
    case imported(Publication, fileName: String)
    /// The library already holds this content; nothing was added.
    case duplicate(Publication, fileName: String)
    /// Added, and its kept reading history came back with it.
    case restored(RestoredBook, fileName: String)
    case failed(ImportFailure)

    var fileName: String {
        switch self {
        case .imported(_, let fileName),
             .duplicate(_, let fileName),
             .restored(_, let fileName): fileName
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

    var restored: [RestoredBook] {
        items.compactMap { if case .restored(let book, _) = $0 { book } else { nil } }
    }

    var failures: [ImportFailure] {
        items.compactMap { if case .failed(let failure) = $0 { failure } else { nil } }
    }

    var isEmpty: Bool { items.isEmpty }

    /// How many books the shelf actually gained. A restore is an import
    /// that also brought a history back, so it counts here exactly like a
    /// plain one — it is on the shelf, and the summary's title says so.
    var addedToLibrary: Int { imported.count + restored.count }

    /// True when the library gained something and screens should reload.
    var didChangeLibrary: Bool { addedToLibrary > 0 }

    /// A run that added everything it was given, with nothing to explain.
    ///
    /// A restore is a success, but not a silent one: the reader was told
    /// their progress would survive removal, and the moment it does is
    /// worth a sentence rather than the same checkmark every import gets.
    var isCleanSuccess: Bool {
        !items.isEmpty && duplicates.isEmpty && failures.isEmpty && restored.isEmpty && !wasCancelled
    }

    /// Anything the user would want itemized — a duplicate they need named,
    /// a book that came back with its history, a file that did not make it
    /// — earns the summary sheet rather than a toast that swallows the
    /// detail.
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
