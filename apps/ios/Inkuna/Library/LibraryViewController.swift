import UIKit

/// The Library tab: search, the Reading/Finished/Wishlist segments, and
/// the book list.
///
/// TODO(core): rows come from `LibraryStore.shared.library` (`Bookshelf`)
/// once import lands — including real shelves for Finished and Wishlist;
/// the list then moves to a diffable collection view.
final class LibraryViewController: ScrollScreenViewController {
    private let listStack = UIStackView()
    private var segment = "Reading"
    private var query = ""

    override func viewDidLoad() {
        super.viewDidLoad()

        // TODO(l10n): localize once the strings pass lands.
        let title = displayTitle("Library")
        contentStack.addArrangedSubview(title)
        contentStack.setCustomSpacing(InkSpacing.space5, after: title)

        let searchField = InkSearchField(placeholder: "Search your library")
        searchField.onTextChange = { [weak self] text in
            self?.query = text
            self?.reloadList()
        }
        contentStack.addArrangedSubview(searchField)
        contentStack.setCustomSpacing(InkSpacing.space4, after: searchField)

        let segmented = InkSegmentedControl(options: ["Reading", "Finished", "Wishlist"], selected: segment)
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
        reloadList()
    }

    private func reloadList() {
        listStack.arrangedSubviews.forEach { $0.removeFromSuperview() }

        // TODO(core): Finished and Wishlist shelves don't exist yet.
        guard segment == "Reading" else {
            let message = segment == "Finished"
                ? "Nothing finished yet. No hurry."
                : "Nothing on the nightstand yet."
            listStack.addArrangedSubview(paddedEmptyState(message))
            return
        }

        let trimmed = query.trimmingCharacters(in: .whitespaces).lowercased()
        let books = PlaceholderLibrary.books.filter { book in
            trimmed.isEmpty || "\(book.title) \(book.author)".lowercased().contains(trimmed)
        }
        guard !books.isEmpty else {
            listStack.addArrangedSubview(paddedEmptyState("Nothing found in the stacks."))
            return
        }
        for book in books {
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
            listStack.addArrangedSubview(row)
        }
    }

    @objc private func openRow(_ recognizer: UITapGestureRecognizer) {
        guard let id = recognizer.view?.tag,
              let book = PlaceholderLibrary.books.first(where: { $0.id == id }) else { return }
        openBook(book)
    }
}
