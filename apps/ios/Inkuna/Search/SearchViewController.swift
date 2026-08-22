import UIKit

/// The Search tab: library search through the core — case-folded, CJK-safe
/// matching over titles and authors, and, beneath them, the core's
/// full-text search inside the books themselves — with a "New this week"
/// shelf of the latest imports while the query is empty.
final class SearchViewController: ScrollScreenViewController {
    /// Books the full-text pass may name; one excerpt each, so this is a
    /// screenful, not a page count.
    private static let fullTextLimit: UInt32 = 20

    private let resultsStack = UIStackView()
    private var query = ""

    /// Rows currently on screen, in the order the core returned them.
    private var publications: [Publication] = []
    /// Books whose text matched, in the core's ranking.
    private var fullTextHits: [LibrarySearchHit] = []
    /// What the last render drew. The routine viewWillAppear reload almost
    /// always comes back identical, and rebuilding identical rows flashes
    /// every cover back in over its placeholder — so an unchanged result
    /// leaves the views standing.
    private var rendered: (rows: [Publication], hits: [LibrarySearchHit], discover: Bool)?
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

        let title = displayTitle(String(localized: "search_title", defaultValue: "Search"))
        contentStack.addArrangedSubview(title)
        contentStack.setCustomSpacing(InkSpacing.space5, after: title)

        let searchField = InkSearchField(placeholder: String(localized: "search_placeholder", defaultValue: "Titles, authors"))
        searchField.onTextChange = { [weak self] text in
            self?.query = text
            self?.reload()
        }
        contentStack.addArrangedSubview(searchField)
        contentStack.setCustomSpacing(InkSpacing.space3, after: searchField)

        #if DEBUG
        // Screenshot-loop hook: the simulator has no keystroke automation,
        // so `-inkuna.debugSearchQuery <q>` types into this screen too.
        if let debugQuery = UserDefaults.standard.string(forKey: "inkuna.debugSearchQuery") {
            searchField.textField.text = debugQuery
            query = debugQuery
        }
        #endif

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
                var hits: [LibrarySearchHit] = []
                if trimmed.isEmpty {
                    // The discover shelf: the freshest imports, a shelf's
                    // worth at most.
                    rows = Array(try await bookshelf.library().list(shelf: .all, sort: .recentlyAdded).prefix(8))
                } else {
                    rows = try await bookshelf.library().searchLibrary(query: trimmed)
                    // The books' own text, under the metadata matches. A
                    // one-letter Latin query matches too much to be worth
                    // reading — one CJK character does not.
                    if ReaderSearchPanel.isSearchable(trimmed) {
                        // Search is derived data: a failed index leaves the
                        // metadata results standing rather than emptying
                        // the screen.
                        hits = (try? await bookshelf.search().searchAllBooks(
                            query: trimmed,
                            limit: Self.fullTextLimit
                        )) ?? []
                    }
                }
                guard !Task.isCancelled, let self else { return }
                self.publications = rows
                self.fullTextHits = hits
                let discover = trimmed.isEmpty
                if let rendered = self.rendered,
                   rendered.rows == rows, rendered.hits == hits, rendered.discover == discover {
                    return
                }
                self.rendered = (rows, hits, discover)
                self.render(rows: rows, discover: discover)
            } catch is CancellationError {
                return
            } catch {
                guard !Task.isCancelled, let self else { return }
                self.publications = []
                self.fullTextHits = []
                self.rendered = nil
                self.renderEmpty(String(localized: "library_unopenable", defaultValue: "The library couldn't be opened."))
            }
        }
    }

    private func render(rows: [Publication], discover: Bool) {
        resultsStack.arrangedSubviews.forEach { $0.removeFromSuperview() }

        guard !rows.isEmpty || !fullTextHits.isEmpty else {
            if discover {
                // An empty library has nothing to discover; the quiet
                // screen leaves the search field standing alone.
                return
            }
            renderEmpty(String(localized: "library_empty_query", defaultValue: "Nothing found in the stacks."))
            return
        }

        if discover {
            let eyebrow = eyebrowLabel(String(localized: "search_recently_added", defaultValue: "Recently added"))
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
            resultsStack.addArrangedSubview(row)
        }

        renderFullText()
    }

    /// The full-text section: the same book row as above, with the core's
    /// excerpt beneath it and the match inked.
    private func renderFullText() {
        guard !fullTextHits.isEmpty else { return }

        let eyebrow = eyebrowLabel(String(localized: "search_in_books", defaultValue: "In your books"))
        let eyebrowWrapper = UIStackView(arrangedSubviews: [eyebrow])
        eyebrowWrapper.axis = .vertical
        eyebrowWrapper.isLayoutMarginsRelativeArrangement = true
        eyebrowWrapper.layoutMargins = UIEdgeInsets(top: InkSpacing.space6, left: 0, bottom: InkSpacing.space2, right: 0)
        resultsStack.addArrangedSubview(eyebrowWrapper)

        for (index, hit) in fullTextHits.enumerated() {
            let publication = hit.publication
            let row = BookListRowView()
            row.configure(
                title: publication.title,
                author: publication.displayAuthors(unknownAuthor: String(localized: "unknown_author", defaultValue: "Unknown author")),
                progress: publication.progression > 0 ? CGFloat(publication.progression) : nil,
                seed: BookCoverView.coverSeed(for: publication.id),
                coverPath: publication.coverPath,
                downloaded: true
            )
            row.isAccessibilityElement = false

            let excerptLabel = InkLabel()
            excerptLabel.attributedText = Self.excerpt(for: hit)
            excerptLabel.isAccessibilityElement = false
            excerptLabel.numberOfLines = 2
            excerptLabel.lineBreakMode = .byTruncatingTail

            let column = UIStackView(arrangedSubviews: [row, excerptLabel])
            column.axis = .vertical
            column.spacing = 2
            column.isLayoutMarginsRelativeArrangement = true
            column.isUserInteractionEnabled = false
            // The excerpt hangs under the row's text, clear of the cover.
            column.layoutMargins = UIEdgeInsets(top: 0, left: 0, bottom: InkSpacing.space3, right: 0)

            let control = UIControl()
            control.tag = index
            control.isAccessibilityElement = true
            let author = publication.displayAuthors(unknownAuthor: String(localized: "unknown_author", defaultValue: "Unknown author"))
            control.accessibilityLabel = [publication.title, author, Self.excerpt(for: hit).string]
                .filter { !$0.isEmpty }
                .joined(separator: ", ")
            control.accessibilityTraits = .button
            control.addAction(UIAction { [weak self, weak control] _ in
                guard let control else { return }
                self?.openFullTextRow(control)
            }, for: .touchUpInside)

            column.translatesAutoresizingMaskIntoConstraints = false
            control.addSubview(column)
            NSLayoutConstraint.activate([
                column.leadingAnchor.constraint(equalTo: control.leadingAnchor),
                column.trailingAnchor.constraint(equalTo: control.trailingAnchor),
                column.topAnchor.constraint(equalTo: control.topAnchor),
                column.bottomAnchor.constraint(equalTo: control.bottomAnchor),
            ])
            resultsStack.addArrangedSubview(control)
        }
    }

    /// The three-piece excerpt as one line of prose with the match inked.
    private static func excerpt(for hit: LibrarySearchHit) -> NSAttributedString {
        let font = InkFont.serif(14, weight: .regular, style: .subheadline)
        let text = NSMutableAttributedString(
            // Head-clamped so the two-line tail truncation can never push
            // the match itself out of the excerpt.
            string: ReaderSearchPanel.clampedLeadingContext(hit.excerptPre),
            attributes: [.font: font, .foregroundColor: InkColor.textSecondary]
        )
        text.append(NSAttributedString(
            string: hit.excerptMatch,
            attributes: [
                .font: InkFont.serif(14, weight: .semibold, style: .subheadline),
                .foregroundColor: InkColor.accentText,
            ]
        ))
        text.append(NSAttributedString(
            string: hit.excerptPost,
            attributes: [.font: font, .foregroundColor: InkColor.textSecondary]
        ))
        return text
    }

    private func renderEmpty(_ message: String) {
        resultsStack.arrangedSubviews.forEach { $0.removeFromSuperview() }
        resultsStack.addArrangedSubview(paddedEmptyState(message))
    }

    @objc private func openRow(_ recognizer: UITapGestureRecognizer) {
        guard let index = recognizer.view?.tag, publications.indices.contains(index) else { return }
        openBook(publications[index])
    }

    @objc private func openFullTextRow(_ control: UIControl) {
        guard fullTextHits.indices.contains(control.tag) else { return }
        openBook(fullTextHits[control.tag].publication)
    }
}
