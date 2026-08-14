import UIKit

/// The Search tab: library search with a "New this week" shelf while the
/// query is empty.
///
/// TODO(core): search goes through the Rust core's CJK-aware index once it
/// lands; the placeholder filter is a plain substring match.
final class SearchViewController: ScrollScreenViewController {
    private let resultsStack = UIStackView()
    private var query = ""

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
        reload()
    }

    private func reload() {
        resultsStack.arrangedSubviews.forEach { $0.removeFromSuperview() }

        let trimmed = query.trimmingCharacters(in: .whitespaces).lowercased()
        guard !trimmed.isEmpty else {
            showDiscoverShelf()
            return
        }

        let results = PlaceholderLibrary.books.filter { book in
            "\(book.title) \(book.author)".lowercased().contains(trimmed)
        }
        guard !results.isEmpty else {
            resultsStack.addArrangedSubview(paddedEmptyState("Nothing found in the stacks."))
            return
        }
        for book in results {
            let row = BookListRowView()
            row.configure(
                title: book.title,
                author: book.author,
                progress: book.progress,
                seed: book.coverSeed,
                downloaded: book.downloaded
            )
            let tap = UITapGestureRecognizer(target: self, action: #selector(openRow(_:)))
            row.addGestureRecognizer(tap)
            row.tag = book.id
            resultsStack.addArrangedSubview(row)
        }
    }

    private func showDiscoverShelf() {
        let eyebrow = eyebrowLabel("New this week")
        let eyebrowWrapper = UIStackView(arrangedSubviews: [eyebrow])
        eyebrowWrapper.axis = .vertical
        eyebrowWrapper.isLayoutMarginsRelativeArrangement = true
        eyebrowWrapper.layoutMargins = UIEdgeInsets(top: InkSpacing.space6, left: 0, bottom: InkSpacing.space4, right: 0)
        resultsStack.addArrangedSubview(eyebrowWrapper)
        resultsStack.addArrangedSubview(shelfRow(books: PlaceholderLibrary.discover))
    }

    @objc private func openRow(_ recognizer: UITapGestureRecognizer) {
        guard let id = recognizer.view?.tag,
              let book = PlaceholderLibrary.books.first(where: { $0.id == id }) else { return }
        openBook(book)
    }
}
