import OSLog
import UIKit

/// Removing a book is the one destructive thing the app does, and it is
/// reachable from two places (the shelf's context menu and the detail
/// screen's overflow menu). Both go through here so the confirmation
/// wording, the haptic, and the library-wide refresh stay identical
/// wherever the user starts from.
@MainActor
enum BookRemoval {
    private static let logger = Logger(subsystem: "app.inkuna.ios", category: "removal")

    /// Presents the destructive confirmation for `publication`. `onRemoved`
    /// runs on the main actor only after the core call succeeds — the
    /// detail screen uses it to pop itself off a book that no longer
    /// exists.
    static func confirm(
        _ publication: Publication,
        from presenter: UIViewController,
        onRemoved: (@MainActor () -> Void)? = nil
    ) {
        let format = NSLocalizedString("remove_confirm_title", comment: "")
        let alert = UIAlertController(
            title: String.localizedStringWithFormat(format, publication.title),
            message: String(
                localized: "remove_confirm_body",
                defaultValue: "Only the book file will be deleted. Your reading progress and bookmarks will be kept."
            ),
            preferredStyle: .alert
        )
        alert.addAction(
            UIAlertAction(
                title: String(localized: "remove_cancel", defaultValue: "Cancel"),
                style: .cancel
            )
        )
        alert.addAction(
            UIAlertAction(
                title: String(localized: "remove_confirm_button", defaultValue: "Remove"),
                style: .destructive
            ) { _ in
                perform(publication, from: presenter, onRemoved: onRemoved)
            }
        )
        presenter.present(alert, animated: true)
    }

    private static func perform(
        _ publication: Publication,
        from presenter: UIViewController,
        onRemoved: (@MainActor () -> Void)?
    ) {
        Task { [id = publication.id] in
            do {
                let bookshelf = try await LibraryStore.shared.library()
                try await bookshelf.library().remove(id: id)
            } catch InkunaError.NotFound {
                // Already gone — a second confirm on a stale row, or a
                // removal that landed from another screen. The user's
                // intent is satisfied either way, so fall through to the
                // refresh rather than blaming them for it.
            } catch {
                logger.warning("remove failed for \(id, privacy: .public): \(error)")
                failed(from: presenter)
                return
            }
            // Every shelf-showing screen observes this rather than being
            // called back, so one post repaints the whole app.
            NotificationCenter.default.post(name: .inkunaLibraryDidChange, object: nil)
            if AppSettings.shared.hapticsEnabled {
                UINotificationFeedbackGenerator().notificationOccurred(.success)
            }
            onRemoved?()
        }
    }

    private static func failed(from presenter: UIViewController) {
        if AppSettings.shared.hapticsEnabled {
            UINotificationFeedbackGenerator().notificationOccurred(.error)
        }
        let alert = UIAlertController(
            title: String(
                localized: "remove_failed",
                defaultValue: "This book couldn't be removed."
            ),
            message: nil,
            preferredStyle: .alert
        )
        alert.addAction(
            UIAlertAction(
                title: String(localized: "remove_dismiss", defaultValue: "OK"),
                style: .default
            )
        )
        presenter.present(alert, animated: true)
    }

    /// The destructive `UIAction` both menus hang off, so the title and
    /// the trash glyph never drift apart.
    ///
    /// `presenter` is captured weakly: the detail screen hands this action
    /// to a button it owns, so a strong capture would close the cycle
    /// screen → button → menu → action → screen and leak a whole view
    /// tree, cover art included, for every book ever opened.
    static func action(
        for publication: Publication,
        from presenter: UIViewController,
        onRemoved: (@MainActor () -> Void)? = nil
    ) -> UIAction {
        UIAction(
            title: String(localized: "remove_action", defaultValue: "Remove from Library"),
            image: UIImage(systemName: "trash"),
            identifier: UIAction.Identifier("remove_action"),
            attributes: .destructive
        ) { [weak presenter] _ in
            guard let presenter else { return }
            confirm(publication, from: presenter, onRemoved: onRemoved)
        }
    }
}
