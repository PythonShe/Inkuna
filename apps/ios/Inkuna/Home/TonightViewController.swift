import ReadiumShared
import UIKit

// The UniFFI bindings are compiled into this target, so `Publication` below
// is the core's record; the Readium import is for `Locator`, which decodes
// the core's stored position — the same shell-side read the detail screen
// does.

/// The Tonight (home) tab: pick up where you left off, filter chips, and
/// the nightstand shelf.
final class TonightViewController: ScrollScreenViewController {
    private var chips: [InkChip] = []

    /// The hero card's slot in the content stack; the card inside is
    /// rebuilt whenever the book to continue changes.
    private let heroContainer = UIView()

    /// What the core last handed back as the most recent unfinished book.
    /// `nil` renders the design's placeholder card — inert scenery, never
    /// a destination.
    private var continueReading: Publication?

    /// Pages left in the chapter the hero book is parked in, when the
    /// stored position and the core's chapter ranges can produce one.
    /// `nil` falls the caption back to the honest percentage.
    private var heroPagesLeft: Int?

    /// The rest of the unfinished shelf, behind the hero. Empty hides the
    /// nightstand section entirely.
    private var nightstand: [Publication] = []
    /// The nightstand section's slot in the content stack (title + shelf),
    /// rebuilt whenever the shelf changes.
    private let shelfSection = UIStackView()

    /// The in-flight reload, cancelled by its successor.
    private var reloadTask: Task<Void, Never>?

    nonisolated(unsafe) private var libraryDidChangeObserver: NSObjectProtocol?

    deinit {
        if let libraryDidChangeObserver {
            NotificationCenter.default.removeObserver(libraryDidChangeObserver)
        }
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        let eyebrow = eyebrowLabel(String(localized: "tonight_eyebrow", defaultValue: "Tonight"))
        contentStack.addArrangedSubview(eyebrow)
        contentStack.setCustomSpacing(6, after: eyebrow)

        let title = displayTitle(String(localized: "tonight_title", defaultValue: "Pick up where you left off"))
        contentStack.addArrangedSubview(title)
        contentStack.setCustomSpacing(28, after: title)

        contentStack.addArrangedSubview(heroContainer)
        contentStack.setCustomSpacing(InkSpacing.space8, after: heroContainer)
        rebuildHero()

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

        shelfSection.axis = .vertical
        contentStack.addArrangedSubview(shelfSection)
        rebuildShelf()

        libraryDidChangeObserver = NotificationCenter.default.addObserver(
            forName: .inkunaLibraryDidChange,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.reloadContinueReading() }
        }
        reloadContinueReading()
    }

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)
        // Progress moves without a library-change notification — coming
        // back from the reader must not show last week's percentage.
        reloadContinueReading()
    }

    // MARK: Continue reading

    private func reloadContinueReading() {
        // One reload at a time: viewWillAppear and a library-change
        // notification can land together, and the older fetch must not
        // repaint over the newer one.
        reloadTask?.cancel()
        reloadTask = Task { [weak self] in
            do {
                let bookshelf = try await LibraryStore.shared.library()
                // Unfinished, not all: a book just finished must not be the
                // "keep reading" hero merely for being touched last.
                let publications = try await bookshelf.list(shelf: .unfinished, sort: .recentlyOpened)
                // The hero's caption wants the chapter it is parked in;
                // the ranges come from the core, the position from the
                // stored locator.
                var pagesLeft: Int?
                if
                    let hero = publications.first,
                    let position = Self.storedPosition(of: hero),
                    let ranges = try? await bookshelf.chapterPositionRanges(id: hero.id)
                {
                    pagesLeft = Self.pagesLeft(in: ranges, at: position)
                }
                guard let self, !Task.isCancelled else { return }
                self.heroPagesLeft = pagesLeft
                // A successful-but-empty answer clears the hero; only a
                // failed load keeps the previous one.
                self.continueReading = publications.first
                // The shelf holds what the hero doesn't: the rest of the
                // unfinished pile, most recently touched first, capped at a
                // shelf's worth.
                self.nightstand = Array(publications.dropFirst().prefix(8))
                self.rebuildHero()
                self.rebuildShelf()
            } catch {
                // Keep whatever the card already shows; the library screen
                // owns the recovery path for a library that will not open.
            }
        }
    }

    /// The synthetic position the book is parked at, read out of the
    /// stored Readium locator — the same shell-side decode the detail
    /// screen does. Nil for a book never opened in a build that reports
    /// positions.
    private static func storedPosition(of publication: Publication) -> UInt32? {
        guard
            let locatorJSON = publication.locator,
            let locator = try? Locator(jsonString: locatorJSON),
            let position = locator.locations.position, position > 0
        else { return nil }
        return UInt32(position)
    }

    /// Pages left in the chapter holding `position`. Chapter ranges are
    /// 1-based and inclusive, and several may contain one position when a
    /// resource carries nested entries — the innermost (greatest start)
    /// wins, the same "closest preceding entry" rule the contents sheet
    /// highlights by. Zero or less is not a caption worth showing.
    private static func pagesLeft(in ranges: [ChapterPositionRange], at position: UInt32) -> Int? {
        let containing = ranges
            .filter { $0.startPosition <= position && position <= $0.endPosition }
            .max { $0.startPosition < $1.startPosition }
        guard let containing else { return nil }
        let left = Int(containing.endPosition) - Int(position)
        return left > 0 ? left : nil
    }

    /// Shows the nightstand only when there is something on it — an empty
    /// core shelf removes the section instead of rendering stand-ins.
    private func rebuildShelf() {
        shelfSection.arrangedSubviews.forEach { $0.removeFromSuperview() }
        guard !nightstand.isEmpty else { return }
        let shelfTitle = sectionTitle(String(localized: "tonight_nightstand", defaultValue: "On the nightstand"))
        shelfSection.addArrangedSubview(shelfTitle)
        shelfSection.setCustomSpacing(InkSpacing.space4, after: shelfTitle)
        shelfSection.addArrangedSubview(shelfRow(publications: nightstand))
    }

    private func rebuildHero() {
        heroContainer.subviews.forEach { $0.removeFromSuperview() }
        let card: UIView
        if let publication = continueReading {
            let percent = Int((publication.progression * 100).rounded())
            let author = publication.displayAuthors(unknownAuthor: String(localized: "unknown_author", defaultValue: "Unknown author"))
            // What is left of the chapter when the core can say, the
            // book-wide percentage when it cannot.
            let caption: String
            if let heroPagesLeft {
                let format = NSLocalizedString("tonight_pages_left_count", comment: "")
                caption = String.localizedStringWithFormat(format, Int64(heroPagesLeft))
            } else {
                let percentFormat = NSLocalizedString("tonight_percent_read", comment: "")
                caption = String.localizedStringWithFormat(percentFormat, Int64(percent))
            }
            card = makeHeroCard(
                title: publication.title,
                author: author,
                progress: CGFloat(publication.progression),
                caption: caption,
                seed: BookCoverView.coverSeed(for: publication.id),
                coverPath: publication.coverPath,
                onOpen: { [weak self] in
                    guard let self, let current = self.continueReading else { return }
                    ReaderLauncher.push(current, on: self.navigationController)
                }
            )
        } else {
            // The design's stand-in book, rendered only while there is
            // nothing to continue — and inert: it opens nothing.
            let book = PlaceholderLibrary.heroBook
            card = makeHeroCard(
                title: book.title,
                author: book.author,
                progress: book.progress,
                caption: PlaceholderLibrary.pagesLeftText,
                seed: book.coverSeed,
                coverPath: nil,
                onOpen: nil
            )
        }
        card.translatesAutoresizingMaskIntoConstraints = false
        heroContainer.addSubview(card)
        NSLayoutConstraint.activate([
            card.leadingAnchor.constraint(equalTo: heroContainer.leadingAnchor),
            card.trailingAnchor.constraint(equalTo: heroContainer.trailingAnchor),
            card.topAnchor.constraint(equalTo: heroContainer.topAnchor),
            card.bottomAnchor.constraint(equalTo: heroContainer.bottomAnchor),
        ])
    }

    // MARK: Hero card

    /// The card shows the book it opens — every value on it comes from the
    /// same source as [onOpen]'s destination, or from the placeholder when
    /// there is no destination at all.
    private func makeHeroCard(
        title: String,
        author: String,
        progress: CGFloat,
        caption: String,
        seed: Int,
        coverPath: String?,
        onOpen: (@MainActor () -> Void)?
    ) -> UIView {
        let card = UIView()
        card.backgroundColor = InkColor.bgSurface
        card.layer.cornerRadius = InkRadius.lg
        card.installInkShadow(.sm)

        let cover = BookCoverView(title: title, author: author, seed: seed, coverPath: coverPath)
        cover.widthAnchor.constraint(equalToConstant: 80).isActive = true

        let titleLabel = InkLabel()
        titleLabel.text = title
        titleLabel.font = InkFont.serif(18, weight: .medium, style: .headline)
        titleLabel.textColor = InkColor.textDisplay
        titleLabel.numberOfLines = 2

        let authorLabel = InkLabel()
        authorLabel.text = author
        authorLabel.font = InkFont.labelRegular
        authorLabel.textColor = InkColor.textSecondary

        let progressBar = InkProgressBar(progress: progress)

        let leftLabel = InkLabel()
        leftLabel.text = caption
        leftLabel.font = InkFont.caption
        leftLabel.textColor = InkColor.textTertiary

        // The button opens the same book as the card; on the placeholder
        // card there is nothing to open and it says so by being disabled.
        let readButton = InkButton(String(localized: "tonight_keep_reading", defaultValue: "Keep reading"), size: .small, symbol: "book") { [weak self] in
            self?.heroOpenAction?()
        }
        readButton.isEnabled = onOpen != nil
        let buttonRow = UIStackView(arrangedSubviews: [readButton, UIView()])
        buttonRow.axis = .horizontal

        let column = UIStackView(arrangedSubviews: [titleLabel, authorLabel, progressBar, leftLabel, buttonRow])
        column.axis = .vertical
        column.spacing = 3
        column.setCustomSpacing(14, after: authorLabel)
        column.setCustomSpacing(InkSpacing.space2, after: progressBar)
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

        // Not a single a11y element: the inner Keep-reading button must stay
        // reachable. The title label doubles as the open affordance.
        titleLabel.accessibilityLabel = "\(title), \(author)"
        if let onOpen {
            heroOpenAction = onOpen
            let tap = UITapGestureRecognizer(target: self, action: #selector(openHero))
            card.addGestureRecognizer(tap)
            titleLabel.isAccessibilityElement = true
            titleLabel.accessibilityTraits = .button
        } else {
            heroOpenAction = nil
            titleLabel.isAccessibilityElement = true
            titleLabel.accessibilityTraits = .staticText
        }
        return card
    }

    private var heroOpenAction: (() -> Void)?

    @objc private func openHero() {
        heroOpenAction?()
    }

    private func pickChip(at index: Int) {
        for (chipIndex, chip) in chips.enumerated() {
            chip.isChosen = chipIndex == index
        }
    }
}
