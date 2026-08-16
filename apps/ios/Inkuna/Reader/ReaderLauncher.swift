import UIKit

/// Opens the reader on a publication from the core library.
///
/// TODO(core): the library, detail, and tonight screens still render
/// placeholder rows with no core identity, so every "keep reading" entry
/// point funnels here and opens the reading position of the most recently
/// opened core publication (falling back to the newest import). Once
/// library wiring lands, those screens pass their own row's `Publication`
/// and this helper keeps only the fetch-and-push mechanics.
@MainActor
enum ReaderLauncher {
    /// Fetches the publication to read and pushes the reader, or explains
    /// quietly why there is nothing to open. Never blocks the main thread —
    /// the core work happens behind an `await`.
    /// Pushes the reader on a publication the caller already holds — the
    /// path every screen that renders real core rows should take.
    static func push(_ publication: Publication, on navigationController: UINavigationController?) {
        guard let navigationController else { return }
        let reader = ReaderViewController(publication: publication)
        reader.hidesBottomBarWhenPushed = true
        navigationController.pushViewController(reader, animated: true)
    }

    static func push(on navigationController: UINavigationController?) {
        guard let navigationController else { return }
        Task {
            do {
                let bookshelf = try await LibraryStore.shared.library()
                let publications = try await bookshelf.list(shelf: .unfinished, sort: .recentlyOpened)
                guard let publication = publications.first else {
                    // Covers both an empty library and one whose every book
                    // is finished — the query is the unfinished shelf.
                    toast(symbol: "books.vertical", text: "Nothing to keep reading right now.", on: navigationController)
                    return
                }
                let reader = ReaderViewController(publication: publication)
                reader.hidesBottomBarWhenPushed = true
                navigationController.pushViewController(reader, animated: true)
            } catch {
                toast(symbol: "exclamationmark.triangle", text: "The library couldn't be opened.", on: navigationController)
            }
        }
    }

    private static func toast(symbol: String, text: String, on navigationController: UINavigationController) {
        guard let view = navigationController.topViewController?.view ?? navigationController.viewIfLoaded else { return }
        // TODO(l10n): localize once the strings pass lands.
        InkToastView.show(
            symbol: symbol,
            text: text,
            in: view,
            topInset: view.safeAreaInsets.top + 56
        )
    }
}
