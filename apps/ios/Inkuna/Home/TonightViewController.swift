import UIKit

/// The Tonight (home) tab: pick up where you left off, filter chips, and
/// the nightstand shelf.
final class TonightViewController: ScrollScreenViewController {
    private var chips: [InkChip] = []

    override func viewDidLoad() {
        super.viewDidLoad()

        // TODO(l10n): localize once the strings pass lands.
        let eyebrow = eyebrowLabel("Tonight")
        contentStack.addArrangedSubview(eyebrow)
        contentStack.setCustomSpacing(6, after: eyebrow)

        let title = displayTitle("Pick up where you left off")
        contentStack.addArrangedSubview(title)
        contentStack.setCustomSpacing(28, after: title)

        let hero = heroCard(for: PlaceholderLibrary.heroBook)
        contentStack.addArrangedSubview(hero)
        contentStack.setCustomSpacing(InkSpacing.space8, after: hero)

        // TODO(core): chips become real library filters once collections
        // exist in the core; selection is cosmetic for now.
        chips = PlaceholderLibrary.tonightChips.enumerated().map { index, label in
            InkChip(label, chosen: index == 0) { [weak self] in
                self?.pickChip(at: index)
            }
        }
        let chipRow = UIStackView(arrangedSubviews: chips + [UIView()])
        chipRow.axis = .horizontal
        chipRow.spacing = InkSpacing.space2
        contentStack.addArrangedSubview(chipRow)
        contentStack.setCustomSpacing(InkSpacing.space4, after: chipRow)

        let shelfTitle = sectionTitle("On the nightstand")
        contentStack.addArrangedSubview(shelfTitle)
        contentStack.setCustomSpacing(InkSpacing.space4, after: shelfTitle)

        contentStack.addArrangedSubview(shelfRow())
    }

    // MARK: Hero card

    private func heroCard(for book: PlaceholderBook) -> UIView {
        let card = UIView()
        card.backgroundColor = InkColor.bgSurface
        card.layer.cornerRadius = InkRadius.xl
        card.installInkShadow(.md)

        let cover = BookCoverView(title: book.title, author: book.author, seed: book.coverSeed)
        cover.widthAnchor.constraint(equalToConstant: 96).isActive = true

        let titleLabel = UILabel()
        titleLabel.text = book.title
        titleLabel.font = InkFont.heading
        titleLabel.textColor = InkColor.textDisplay
        titleLabel.numberOfLines = 2

        let authorLabel = UILabel()
        authorLabel.text = book.author
        authorLabel.font = InkFont.labelRegular
        authorLabel.textColor = InkColor.textSecondary

        let progress = InkProgressBar(progress: book.progress)

        let leftLabel = UILabel()
        leftLabel.text = PlaceholderLibrary.pagesLeftText
        leftLabel.font = InkFont.caption
        leftLabel.textColor = InkColor.textTertiary

        let readButton = InkButton("Keep reading", size: .small, symbol: "book") { [weak self] in
            self?.openReader(PlaceholderLibrary.heroBook)
        }
        let buttonRow = UIStackView(arrangedSubviews: [readButton, UIView()])
        buttonRow.axis = .horizontal

        let column = UIStackView(arrangedSubviews: [titleLabel, authorLabel, progress, leftLabel, buttonRow])
        column.axis = .vertical
        column.spacing = 3
        column.setCustomSpacing(14, after: authorLabel)
        column.setCustomSpacing(InkSpacing.space2, after: progress)
        column.setCustomSpacing(14, after: leftLabel)

        let row = UIStackView(arrangedSubviews: [cover, column])
        row.axis = .horizontal
        row.spacing = 18
        row.alignment = .bottom
        row.translatesAutoresizingMaskIntoConstraints = false
        card.addSubview(row)
        NSLayoutConstraint.activate([
            row.leadingAnchor.constraint(equalTo: card.leadingAnchor, constant: InkSpacing.space5),
            row.trailingAnchor.constraint(equalTo: card.trailingAnchor, constant: -InkSpacing.space5),
            row.topAnchor.constraint(equalTo: card.topAnchor, constant: InkSpacing.space5),
            row.bottomAnchor.constraint(equalTo: card.bottomAnchor, constant: -InkSpacing.space5),
        ])

        let tap = UITapGestureRecognizer(target: self, action: #selector(openHeroDetail))
        card.addGestureRecognizer(tap)
        return card
    }

    @objc private func openHeroDetail() {
        openBook(PlaceholderLibrary.heroBook)
    }

    // MARK: Shelf

    private func shelfRow() -> UIView {
        let shelfScroll = UIScrollView()
        shelfScroll.showsHorizontalScrollIndicator = false
        shelfScroll.clipsToBounds = false

        let shelf = UIStackView()
        shelf.axis = .horizontal
        shelf.spacing = InkSpacing.space4
        shelf.alignment = .top
        shelf.translatesAutoresizingMaskIntoConstraints = false
        shelfScroll.addSubview(shelf)

        for book in PlaceholderLibrary.shelf {
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
        return shelfScroll
    }

    private func pickChip(at index: Int) {
        for (chipIndex, chip) in chips.enumerated() {
            chip.isChosen = chipIndex == index
        }
    }
}
