import Foundation

/// Every user-visible string the import flow can show, in one place.
///
/// The house rule is to keep literals out of deep code so the l10n pass
/// stays mechanical; for a feature whose whole job is explaining outcomes,
/// one table beats sprinkling them across four view controllers.
enum ImportCopy {
    // MARK: Empty library

    static var emptyTitle: String {
        String(localized: "import_empty_title", defaultValue: "Nothing on the shelf yet")
    }

    static var emptyBody: String {
        String(
            localized: "import_empty_body",
            defaultValue: "Add a book from Files, iCloud Drive, or wherever your books live. Inkuna keeps its own copy, so the original stays where it is."
        )
    }

    static var emptyFootnote: String {
        String(localized: "import_empty_footnote", defaultValue: "EPUB, MOBI, AZW3, and TXT today. Comics to come.")
    }

    static var addBooks: String {
        String(localized: "import_add_books", defaultValue: "Add books")
    }

    // MARK: Progress

    static var preparing: String {
        String(localized: "import_phase_preparing", defaultValue: "Getting the file ready…")
    }

    static func progressTitle(total: Int) -> String {
        if total > 1 {
            let format = NSLocalizedString("import_progress_title_many", comment: "")
            return String.localizedStringWithFormat(format, books(total))
        } else {
            return String(localized: "import_progress_title_one", defaultValue: "Adding to your library")
        }
    }

    static func progressCount(completed: Int, total: Int) -> String {
        let format = NSLocalizedString("import_progress_count", comment: "")
        return String.localizedStringWithFormat(format, Int64(completed), Int64(total))
    }

    static var cancel: String {
        String(localized: "import_cancel", defaultValue: "Cancel")
    }

    // MARK: Toasts and alerts

    static func addedOne(_ title: String) -> String {
        let format = NSLocalizedString("import_toast_added_one", comment: "")
        return String.localizedStringWithFormat(format, title)
    }

    static func addedMany(_ count: Int) -> String {
        let format = NSLocalizedString("import_result_added", comment: "")
        return String.localizedStringWithFormat(format, Int64(count))
    }

    static func alreadyInLibrary(_ title: String) -> String {
        let format = NSLocalizedString("import_toast_already", comment: "")
        return String.localizedStringWithFormat(format, title)
    }

    static func couldNotAdd(_ fileName: String) -> String {
        let format = NSLocalizedString("import_toast_could_not_add", comment: "")
        return String.localizedStringWithFormat(format, fileName)
    }

    static var nothingAdded: String {
        String(localized: "import_result_nothing", defaultValue: "Nothing added")
    }

    static var dismiss: String {
        String(localized: "import_dismiss", defaultValue: "OK")
    }

    static var done: String {
        String(localized: "import_done", defaultValue: "Done")
    }

    // MARK: Summary sheet

    static func summaryTitle(_ report: ImportReport) -> String {
        let added = report.imported.count
        return added > 0 ? addedMany(added) : nothingAdded
    }

    /// The line under the sheet title: what needs saying beyond the count.
    static func summarySubtitle(_ report: ImportReport) -> String? {
        var parts: [String] = []
        if report.duplicates.count == 1 {
            parts.append(String(localized: "import_subtitle_duplicate_one", defaultValue: "1 was already in your library"))
        } else if report.duplicates.count > 1 {
            let format = NSLocalizedString("import_subtitle_duplicate_many", comment: "")
            parts.append(String.localizedStringWithFormat(format, Int64(report.duplicates.count)))
        }
        if report.failures.count == 1 {
            parts.append(String(localized: "import_subtitle_failure_one", defaultValue: "1 couldn't be added"))
        } else if report.failures.count > 1 {
            let format = NSLocalizedString("import_subtitle_failure_many", comment: "")
            parts.append(String.localizedStringWithFormat(format, Int64(report.failures.count)))
        }
        if report.wasCancelled {
            parts.append(String(localized: "import_subtitle_cancelled", defaultValue: "you stopped the rest"))
        }
        guard !parts.isEmpty else { return nil }
        return parts.joined(separator: " · ")
    }

    static var statusAdded: String {
        String(localized: "import_status_added", defaultValue: "Added to your library")
    }

    static var statusDuplicate: String {
        String(localized: "import_status_duplicate", defaultValue: "Already in your library")
    }

    // MARK: Failures

    /// The one sentence a reader gets for a file that did not make it.
    /// Honest about the class of problem, and about what is coming: the
    /// core names the format it detected precisely so this can promise the
    /// format rather than apologize for an error.
    static func explanation(_ reason: ImportFailureReason) -> String {
        switch reason {
        case .unsupportedFormat(let name):
            if let name, let known = KnownFormat(tag: name) {
                let format = NSLocalizedString("import_fail_unsupported", comment: "")
                return String.localizedStringWithFormat(format, known.display)
            } else if let name {
                let format = NSLocalizedString("import_fail_unsupported", comment: "")
                return String.localizedStringWithFormat(format, name.uppercased(with: .current))
            } else {
                return String(localized: "import_fail_unsupported_generic", defaultValue: "This isn't a book file Inkuna recognizes.")
            }
        case .damagedArchive:
            return String(localized: "import_fail_archive", defaultValue: "The file is damaged and couldn't be opened.")
        case .invalidPublication:
            return String(localized: "import_fail_broken", defaultValue: "This book can't be opened. It may be incomplete — or protected by DRM, which Inkuna doesn't support.")
        case .tooLarge:
            return String(localized: "import_fail_too_large", defaultValue: "This file is too large for Inkuna to add.")
        case .storage:
            return String(localized: "import_fail_storage", defaultValue: "The file couldn't be read. Check that it's still available and that this device has room.")
        case .database:
            return String(localized: "import_fail_library", defaultValue: "Your library couldn't be written to.")
        case .notFound:
            return String(localized: "import_fail_not_found", defaultValue: "The book went missing from the library mid-import.")
        case .unreadableSource:
            return String(localized: "import_fail_unreadable", defaultValue: "Inkuna couldn't get permission to read this file.")
        case .libraryUnavailable:
            return String(localized: "import_fail_library_unavailable", defaultValue: "Your library wouldn't open, so nothing was added.")
        case .unknown(let message):
            let format = NSLocalizedString("import_fail_unknown", comment: "")
            return String.localizedStringWithFormat(format, message)
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
            case .txt: String(localized: "import_format_plain_text", defaultValue: "Plain text")
            case .pdf: "PDF"
            case .cbz: String(localized: "import_format_cbz", defaultValue: "CBZ comic")
            case .cbr: String(localized: "import_format_cbr", defaultValue: "CBR comic")
            }
        }
    }

    // MARK: Helpers

    static func books(_ count: Int) -> String {
        let format = NSLocalizedString("import_books_count", comment: "")
        return String.localizedStringWithFormat(format, Int64(count))
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
