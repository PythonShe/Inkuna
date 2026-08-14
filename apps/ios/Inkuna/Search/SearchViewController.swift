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
            let label = emptyStateLabel("Nothing found in the stacks.")
            let wrapper = UIStackView(arrangedSubviews: [label])
            wrapper.axis = .vertical
            wrapper.isLayoutMarginsRelativeArrangement = true
            wrapper.layoutMargins = UIEdgeInsets(top: InkSpacing.space12, left: 0, bottom: InkSpacing.space12, right: 0)
            resultsStack.addArrangedSubview(wrapper)
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

        let shelfScroll = UIScrollView()
        shelfScroll.showsHorizontalScrollIndicator = false
        shelfScroll.clipsToBounds = false

        let shelf = UIStackView()
        shelf.axis = .horizontal
        shelf.spacing = InkSpacing.space4
        shelf.alignment = .top
        shelf.translatesAutoresizingMaskIntoConstraints = false
        shelfScroll.addSubview(shelf)

        for book in PlaceholderLibrary.discover {
            let item = ShelfBookView(book: book)
            item.onOpen = { [weak self] book in self?.openBook(book) }
            shelf.addArrangedSubview(item)
        }

        NSLayoutConstraint.activate([
            shelf.leadingAnchor.constraint(equalTo: shelfScroll.contentLayoutGuide.leadingAnchor),
            shelf.trailingAnchor.constraint(equalTo: shelfScroll.contentLayoutGuide.trailingAnchor),
            shelf.topAnchor.constraint(equalTo: shelfScroll.contentLayoutGuide.topAnchor),
            shelf.bottomAnchor.constraint(equalTo: shelfScroll.contentLayoutGuide.bottomAnchor),
            shelfScroll.heightAnchor.constraint(equalTo: shelf.heightAnchor),
        ])
        resultsStack.addArrangedSubview(shelfScroll)
    }

    @objc private func openRow(_ recognizer: UITapGestureRecognizer) {
        guard let id = recognizer.view?.tag,
              let book = PlaceholderLibrary.books.first(where: { $0.id == id }) else { return }
        openBook(book)
    }
}
