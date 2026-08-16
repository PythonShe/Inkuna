import UIKit

/// The Library tab: search, the Reading/Finished/Wishlist segments, and
/// the book list — all of it rendered from the Rust core.
///
/// Shelf membership is the core's decision, never this screen's: each
/// segment asks for its own `Shelf` rather than fetching everything and
/// re-deriving "reading" or "finished" in Swift, which would duplicate core
/// logic and drift from it.
///
/// TODO(core): the Wishlist segment has no core shelf yet (file-less
/// publications are deferred to their own spec), so it stays visible and
/// empty. The list also still moves to a diffable collection view once row
/// churn justifies it.
final class LibraryViewController: ScrollScreenViewController {
    private let listStack = UIStackView()
    private var segment = "Reading"
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

        let title = displayTitle(String(localized: "library_title", defaultValue: "Library"))
        // The import affordance rides beside the title: this screen hides
        // the navigation bar, so there is no bar button item to put it in.
        let addButton = InkIconButton(symbol: "plus", accessibilityLabel: String(localized: "import_add_books", defaultValue: "Add books")) { [weak self] in
            guard let self else { return }
            ImportFlow.presentPicker(from: self)
        }
        let titleRow = UIStackView(arrangedSubviews: [title, UIView(), addButton])
        titleRow.axis = .horizontal
        titleRow.alignment = .center
        contentStack.addArrangedSubview(titleRow)
        contentStack.setCustomSpacing(InkSpacing.space5, after: titleRow)

        let searchField = InkSearchField(placeholder: String(localized: "library_search_placeholder", defaultValue: "Search your library"))
        searchField.onTextChange = { [weak self] text in
            self?.query = text
            self?.reloadList()
        }
        contentStack.addArrangedSubview(searchField)
        contentStack.setCustomSpacing(InkSpacing.space4, after: searchField)

        let segmented = InkSegmentedControl(
            options: [
                String(localized: "library_seg_reading", defaultValue: "Reading"),
                String(localized: "library_seg_finished", defaultValue: "Finished"),
                String(localized: "library_seg_wishlist", defaultValue: "Wishlist"),
            ],
            selected: segment
        )
        segmented.onChange = { [weak self] option in
            self?.segment = option
            self?.reloadList()
        }
        let segmentRow = UIStackView(arrangedSubviews: [segmented, UIView()])
        segmentRow.axis = .horizontal
        contentStack.addArrangedSubview(segmentRow)
        contentStack.setCustomSpacing(InkSpacing.space2, after: segmentRow)

        listStack.axis = .vertical
        contentStack.addArrangedSubview(listStack)

        // An import from anywhere — this screen's picker, "Open with
        // Inkuna", the share sheet — refreshes the list.
        libraryDidChangeObserver = NotificationCenter.default.addObserver(
            forName: .inkunaLibraryDidChange,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.reloadList() }
        }

        reloadList()
    }

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)
        // Progress moves while the reader is open; the rows behind it are
        // stale by the time the reader is popped.
        reloadList()
    }

    /// Fetches the current segment from the core and rebuilds the list.
    ///
    /// Every core call is awaited, so the main thread is never blocked on
    /// SQLite — the list simply keeps its previous rows until the new ones
    /// arrive, which reads as instant for any realistic library.
    private func reloadList() {
        reloadTask?.cancel()

        // TODO(core): no Wishlist shelf exists yet — file-less publications
        // are deferred to their own spec, so the segment stays empty.
        guard segment != "Wishlist" else {
            publications = []
            render(rows: [], emptiness: .shelf("Nothing on the nightstand yet."))
            return
        }

        // Unfinished, not Reading: a book must be listed the moment it is
        // imported, and Reading deliberately means "opened at least once".
        let shelf: Shelf = segment == "Finished" ? .finished : .unfinished
        let trimmed = query.trimmingCharacters(in: .whitespaces)

        reloadTask = Task { [weak self] in
            do {
                // Typing is not a query. A keystroke's reload waits out the
                // burst before touching the core; the next keystroke cancels
                // this sleep, so a fast typist costs one search, not eight.
                // Every reload with a query present waits — only an empty
                // field (appearing, switching segments, library changes
                // included) repaints at once.
                if !trimmed.isEmpty {
                    try await Task.sleep(for: .milliseconds(200))
                }
                let bookshelf = try await LibraryStore.shared.library()
                let rows: [Publication]
                if trimmed.isEmpty {
                    rows = try await bookshelf.list(shelf: shelf, sort: .recentlyOpened)
                } else {
                    // Metadata search is the core's, CJK-aware segmentation
                    // and all. The shelf still filters the result, so a
                    // search inside Finished stays inside Finished.
                    let matches = try await bookshelf.searchLibrary(query: trimmed)
                    let shelved = Set(
                        try await bookshelf.list(shelf: shelf, sort: .recentlyOpened).map(\.id)
                    )
                    rows = matches.filter { shelved.contains($0.id) }
                }
                guard !Task.isCancelled else { return }

                // Distinguish "this shelf is empty" from "there are no
                // books at all" — only the latter earns the invitation to
                // import, and it costs a second query only when empty.
                var emptiness: Emptiness = .shelf(
                    shelf == .finished
                        ? String(localized: "library_empty_finished", defaultValue: "Nothing finished yet. No hurry.")
                        : String(localized: "library_empty_reading", defaultValue: "Nothing here yet.")
                )
                if rows.isEmpty {
                    if !trimmed.isEmpty {
                        emptiness = .shelf(String(localized: "library_empty_query", defaultValue: "Nothing found in the stacks."))
                    } else if try await bookshelf.list(shelf: .all, sort: .recentlyAdded).isEmpty {
                        emptiness = .wholeLibrary
                    }
                }
                guard !Task.isCancelled else { return }
                self?.publications = rows
                self?.render(rows: rows, emptiness: emptiness)
            } catch is CancellationError {
                return
            } catch {
                guard !Task.isCancelled else { return }
                // A library that will not open is worth saying plainly
                // rather than showing as an empty shelf.
                self?.publications = []
                self?.render(rows: [], emptiness: .shelf(String(localized: "library_unopenable", defaultValue: "The library couldn't be opened.")))
            }
        }
    }

    /// What to show when there is nothing to list.
    private enum Emptiness {
        /// This shelf or search has no rows, but the library has books.
        case shelf(String)
        /// The library holds no books at all.
        case wholeLibrary
    }

    private func render(rows: [Publication], emptiness: Emptiness) {
        listStack.arrangedSubviews.forEach { $0.removeFromSuperview() }

        guard !rows.isEmpty else {
            switch emptiness {
            case .shelf(let message):
                listStack.addArrangedSubview(paddedEmptyState(message))
            case .wholeLibrary:
                let invitation = EmptyLibraryView.inviting(self)
                listStack.addArrangedSubview(invitation)
                invitation.appear()
            }
            return
        }

        for (index, publication) in rows.enumerated() {
            let row = BookListRowView()
            row.configure(
                title: publication.title,
                author: publication.displayAuthors(unknownAuthor: String(localized: "unknown_author", defaultValue: "Unknown author")),
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
            listStack.addArrangedSubview(row)
        }
    }

    @objc private func openRow(_ recognizer: UITapGestureRecognizer) {
        guard let index = recognizer.view?.tag, publications.indices.contains(index) else { return }
        openBook(publications[index])
    }
}
