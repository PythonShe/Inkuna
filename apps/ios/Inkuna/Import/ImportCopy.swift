import Foundation

/// Every user-visible string the import flow can show, in one place.
///
/// The house rule is to keep literals out of deep code so the l10n pass
/// stays mechanical; for a feature whose whole job is explaining outcomes,
/// one table beats sprinkling them across four view controllers.
///
// TODO(l10n): move to a `.strings` catalog once the strings pass lands.
// The counted strings need `.stringsdict` plurals; `books(_:)` below is a
// deliberate placeholder, not a pluralization strategy.
enum ImportCopy {
    // MARK: Empty library

    static let emptyTitle = "Nothing on the shelf yet"
    static let emptyBody = """
        Add an EPUB from Files, iCloud Drive, or wherever your books live. \
        Inkuna keeps its own copy, so the original stays where it is.
        """
    static let emptyFootnote = "EPUB today. Comics and more to come."
    static let addBooks = "Add books"

    // MARK: Progress

    static let preparing = "Getting the file ready…"
    static func progressTitle(total: Int) -> String {
        total > 1 ? "Adding \(books(total))" : "Adding to your library"
    }

    static func progressCount(completed: Int, total: Int) -> String {
        "\(completed) of \(total)"
    }

    static let cancel = "Cancel"

    // MARK: Toasts and alerts

    static func addedOne(_ title: String) -> String { "Added \(title)" }
    static func addedMany(_ count: Int) -> String { "Added \(books(count))" }
    static func alreadyInLibrary(_ title: String) -> String { "\(title) is already in your library" }
    static func couldNotAdd(_ fileName: String) -> String { "Couldn't add \(fileName)" }
    static let nothingAdded = "Nothing added"
    static let dismiss = "OK"
    static let done = "Done"

    // MARK: Summary sheet

    static func summaryTitle(_ report: ImportReport) -> String {
        let added = report.imported.count
        return added > 0 ? addedMany(added) : nothingAdded
    }

    /// The line under the sheet title: what needs saying beyond the count.
    static func summarySubtitle(_ report: ImportReport) -> String? {
        var parts: [String] = []
        if report.duplicates.count == 1 {
            parts.append("1 was already in your library")
        } else if report.duplicates.count > 1 {
            parts.append("\(report.duplicates.count) were already in your library")
        }
        if report.failures.count == 1 {
            parts.append("1 couldn't be added")
        } else if report.failures.count > 1 {
            parts.append("\(report.failures.count) couldn't be added")
        }
        if report.wasCancelled {
            parts.append("you stopped the rest")
        }
        guard !parts.isEmpty else { return nil }
        return parts.joined(separator: " · ")
    }

    static let statusAdded = "Added to your library"
    static let statusDuplicate = "Already in your library"

    // MARK: Failures

    /// The one sentence a reader gets for a file that did not make it.
    /// Honest about the class of problem, and about what is coming: the
    /// core names the format it detected precisely so this can promise the
    /// format rather than apologize for an error.
    static func explanation(_ reason: ImportFailureReason) -> String {
        switch reason {
        case .unsupportedFormat(let name):
            if let name, let known = KnownFormat(tag: name) {
                "\(known.display) files aren't supported yet — Inkuna reads EPUB today."
            } else if let name {
                "\(name.uppercased(with: .current)) files aren't supported — Inkuna reads EPUB today."
            } else {
                "This isn't a book file Inkuna recognizes."
            }
        case .damagedArchive:
            "The file is damaged and couldn't be opened."
        case .invalidPublication:
            "This EPUB is missing something Inkuna needs to read it."
        case .storage:
            "The file couldn't be read. Check that it's still available and that this device has room."
        case .database:
            "Your library couldn't be written to."
        case .notFound:
            "The book went missing from the library mid-import."
        case .unreadableSource:
            "Inkuna couldn't get permission to read this file."
        case .libraryUnavailable:
            "Your library wouldn't open, so nothing was added."
        case .unknown(let message):
            "Something went wrong: \(message)"
        }
    }

    /// Formats the core's lowercase format tags the way a reader writes
    /// them. Unknown tags fall back to plain uppercasing rather than being
    /// hidden, so a format a newer core detects still gets named.
    private enum KnownFormat: String {
        case epub, mobi, azw3, txt, pdf, cbz, cbr

        init?(tag: String) {
            self.init(rawValue: tag.lowercased())
        }

        var display: String {
            switch self {
            case .epub: "EPUB"
            case .mobi: "MOBI"
            case .azw3: "AZW3"
            case .txt: "Plain text"
            case .pdf: "PDF"
            case .cbz: "CBZ comic"
            case .cbr: "CBR comic"
            }
        }
    }

    // MARK: Helpers

    static func books(_ count: Int) -> String {
        count == 1 ? "1 book" : "\(count) books"
    }

    /// Trims a title to something a one-line toast can hold without
    /// stretching past the screen. Counts grapheme clusters, so a CJK title
    /// is measured in characters a reader would count and an emoji is never
    /// cut in half.
    static func shortened(_ title: String, limit: Int = 24) -> String {
        let characters = title.count
        guard characters > limit else { return title }
        return String(title.prefix(limit - 1)) + "…"
    }
}
