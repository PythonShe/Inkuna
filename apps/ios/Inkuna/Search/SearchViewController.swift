import UIKit

/// The Search tab: library search through the core — case-folded, CJK-safe
/// matching over titles and authors — with a "New this week" shelf of the
/// latest imports while the query is empty.
final class SearchViewController: ScrollScreenViewController {
    private let resultsStack = UIStackView()
    private var query = ""

    /// Rows currently on screen, in the order the core returned them.
    private var publications: [Publication] = []
    /// The in-flight fetch. Held so a fast typist's keystrokes cancel their
    /// predecessors instead of racing each other onto the list.
    private var reloadTask: Task<Void, Never>?
    /// `nonisolated(unsafe)` so the nonisolated `deinit` can unregister it.
    /// Only ever touched on the main actor: assigned in `viewDidLoad`, read
    /// once at deinit, when no other reference to this screen survives.
    nonisolated(unsafe) private var libraryDidChangeObserver: NSObjectProtocol?

    deinit {
        if let libraryDidChangeObserver {
            NotificationCenter.default.removeObserver(libraryDidChangeObserver)
        }
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        // TODO(l10n): localize once the strings pass lands.
        let title = displayTitle("Search")
        contentStack.addArrangedSubview(title)
        contentStack.setCustomSpacing(InkSpacing.space5, after: title)

        let searchField = InkSearchField(placeholder: "Titles, authors")
        searchField.onTextChange = { [weak self] text in
            self?.query = text
            self?.reload()
        }
        contentStack.addArrangedSubview(searchField)
        contentStack.setCustomSpacing(InkSpacing.space3, after: searchField)

        resultsStack.axis = .vertical
        contentStack.addArrangedSubview(resultsStack)

        // An import from anywhere refreshes the discover shelf.
        libraryDidChangeObserver = NotificationCenter.default.addObserver(
            forName: .inkunaLibraryDidChange,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.reload() }
        }

        reload()
    }

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)
        // Progress moves while the reader is open; result rows behind it
        // are stale by the time it is popped.
        reload()
    }

    private func reload() {
        reloadTask?.cancel()

        let trimmed = query.trimmingCharacters(in: .whitespaces)
        reloadTask = Task { [weak self] in
            do {
                // Typing is not a query: a keystroke's search waits out the
                // burst, and the next keystroke cancels this sleep — the
                // same debounce as the library screen.
                if !trimmed.isEmpty {
                    try await Task.sleep(for: .milliseconds(200))
                }
                let bookshelf = try await LibraryStore.shared.library()
                let rows: [Publication]
                if trimmed.isEmpty {
                    // The discover shelf: the freshest imports, a shelf's
                    // worth at most.
                    rows = Array(try await bookshelf.list(shelf: .all, sort: .recentlyAdded).prefix(8))
                } else {
                    rows = try await bookshelf.searchLibrary(query: trimmed)
                }
                guard !Task.isCancelled, let self else { return }
                self.publications = rows
                self.render(rows: rows, discover: trimmed.isEmpty)
            } catch is CancellationError {
                return
            } catch {
                guard !Task.isCancelled, let self else { return }
                self.publications = []
                // TODO(l10n): localize once the strings pass lands.
                self.renderEmpty("The library couldn't be opened.")
            }
        }
    }

    private func render(rows: [Publication], discover: Bool) {
        resultsStack.arrangedSubviews.forEach { $0.removeFromSuperview() }

        guard !rows.isEmpty else {
            if discover {
                // An empty library has nothing to discover; the quiet
                // screen leaves the search field standing alone.
                return
            }
            // TODO(l10n): localize once the strings pass lands.
            renderEmpty("Nothing found in the stacks.")
            return
        }

        if discover {
            // TODO(l10n): localize once the strings pass lands.
            let eyebrow = eyebrowLabel("New this week")
            let eyebrowWrapper = UIStackView(arrangedSubviews: [eyebrow])
            eyebrowWrapper.axis = .vertical
            eyebrowWrapper.isLayoutMarginsRelativeArrangement = true
            eyebrowWrapper.layoutMargins = UIEdgeInsets(top: InkSpacing.space6, left: 0, bottom: InkSpacing.space4, right: 0)
            resultsStack.addArrangedSubview(eyebrowWrapper)
            resultsStack.addArrangedSubview(shelfRow(publications: rows))
            return
        }

        for (index, publication) in rows.enumerated() {
            let row = BookListRowView()
            row.configure(
                title: publication.title,
                author: publication.displayAuthors,
                progress: publication.progression > 0 ? CGFloat(publication.progression) : nil,
                seed: BookCoverView.coverSeed(for: publication.id),
                coverPath: publication.coverPath,
                // The core owns every book's file, so a listed book is
                // always on disk — there is no cloud-only state to badge.
                downloaded: true
            )
            let tap = UITapGestureRecognizer(target: self, action: #selector(openRow(_:)))
            row.addGestureRecognizer(tap)
            row.tag = index
            resultsStack.addArrangedSubview(row)
        }
    }

    private func renderEmpty(_ message: String) {
        resultsStack.arrangedSubviews.forEach { $0.removeFromSuperview() }
        resultsStack.addArrangedSubview(paddedEmptyState(message))
    }

    @objc private func openRow(_ recognizer: UITapGestureRecognizer) {
        guard let index = recognizer.view?.tag, publications.indices.contains(index) else { return }
        openBook(publications[index])
    }
}
