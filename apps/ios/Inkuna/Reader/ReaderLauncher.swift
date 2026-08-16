import UIKit

/// Opens the reader on a publication from the core library.
@MainActor
enum ReaderLauncher {
    /// Pushes the reader on a publication the caller already holds — the
    /// path every screen takes. `startingAt` opens at that chapter instead
    /// of the saved position (a detail-screen contents row).
    static func push(
        _ publication: Publication,
        startingAt chapter: Chapter? = nil,
        on navigationController: UINavigationController?
    ) {
        guard let navigationController else { return }
        let reader = ReaderViewController(publication: publication, initialChapter: chapter)
        reader.hidesBottomBarWhenPushed = true
        navigationController.pushViewController(reader, animated: true)
    }

    #if DEBUG
    /// Fetches the most recently opened unfinished publication and pushes
    /// the reader on it — the `-inkuna.debugScreen reader` deep launch,
    /// which starts with no publication in hand. Never blocks the main
    /// thread; the core work happens behind an `await`.
    static func push(on navigationController: UINavigationController?) {
        guard let navigationController else { return }
        Task {
            do {
                let bookshelf = try await LibraryStore.shared.library()
                let publications = try await bookshelf.list(shelf: .unfinished, sort: .recentlyOpened)
                guard let publication = publications.first else {
                    // Covers both an empty library and one whose every book
                    // is finished — the query is the unfinished shelf.
                    toast(symbol: "books.vertical", text: String(localized: "library_empty_reading", defaultValue: "Nothing to keep reading right now."), on: navigationController)
                    return
                }
                let reader = ReaderViewController(publication: publication)
                reader.hidesBottomBarWhenPushed = true
                navigationController.pushViewController(reader, animated: true)
            } catch {
                toast(symbol: "exclamationmark.triangle", text: String(localized: "library_unopenable", defaultValue: "The library couldn't be opened."), on: navigationController)
            }
        }
    }

    private static func toast(symbol: String, text: String, on navigationController: UINavigationController) {
        guard let view = navigationController.topViewController?.view ?? navigationController.viewIfLoaded else { return }
        InkToastView.show(
            symbol: symbol,
            text: text,
            in: view,
            topInset: view.safeAreaInsets.top + 56
        )
    }
    #endif
}
