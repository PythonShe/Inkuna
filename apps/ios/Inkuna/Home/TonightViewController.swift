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

        contentStack.addArrangedSubview(shelfRow(books: PlaceholderLibrary.shelf))
    }

    // MARK: Hero card

    private func heroCard(for book: PlaceholderBook) -> UIView {
        let card = UIView()
        card.backgroundColor = InkColor.bgSurface
        card.layer.cornerRadius = InkRadius.xl
        card.installInkShadow(.md)

        let cover = BookCoverView(title: book.title, author: book.author, seed: book.coverSeed)
        cover.widthAnchor.constraint(equalToConstant: 96).isActive = true

        let titleLabel = InkLabel()
        titleLabel.text = book.title
        titleLabel.font = InkFont.heading
        titleLabel.textColor = InkColor.textDisplay
        titleLabel.numberOfLines = 2

        let authorLabel = InkLabel()
        authorLabel.text = book.author
        authorLabel.font = InkFont.labelRegular
        authorLabel.textColor = InkColor.textSecondary

        let progress = InkProgressBar(progress: book.progress)

        let leftLabel = InkLabel()
        leftLabel.text = PlaceholderLibrary.pagesLeftText
        leftLabel.font = InkFont.caption
        leftLabel.textColor = InkColor.textTertiary

        let readButton = InkButton("Keep reading", size: .small, symbol: "book") { [weak self] in
            self?.openReader()
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
        // Not a single a11y element: the inner Keep-reading button must stay
        // reachable. The title label doubles as the detail affordance.
        titleLabel.isAccessibilityElement = true
        titleLabel.accessibilityLabel = "\(book.title), \(book.author)"
        titleLabel.accessibilityTraits = .button
        return card
    }

    @objc private func openHeroDetail() {
        openBook(PlaceholderLibrary.heroBook)
    }

    private func pickChip(at index: Int) {
        for (chipIndex, chip) in chips.enumerated() {
            chip.isChosen = chipIndex == index
        }
    }
}
