import UIKit

/// Book detail: the cover held at arm's length, progress, and the table
/// of contents with the reader's current chapter inked in accent.
///
/// TODO(core): chapters and progress come from the Rust core's metadata /
/// TOC extraction; the placeholder contents render the hero book for now.
final class BookDetailViewController: UIViewController {
    private let book: PlaceholderBook

    init(book: PlaceholderBook) {
        self.book = book
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = InkColor.bgApp

        let scrollView = UIScrollView()
        scrollView.alwaysBounceVertical = true
        scrollView.showsVerticalScrollIndicator = false
        scrollView.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(scrollView)

        let content = UIStackView()
        content.axis = .vertical
        content.translatesAutoresizingMaskIntoConstraints = false
        scrollView.addSubview(content)

        NSLayoutConstraint.activate([
            scrollView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            scrollView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            scrollView.topAnchor.constraint(equalTo: view.topAnchor),
            scrollView.bottomAnchor.constraint(equalTo: view.bottomAnchor),
            content.leadingAnchor.constraint(equalTo: scrollView.contentLayoutGuide.leadingAnchor, constant: InkSpacing.pageMargin),
            content.trailingAnchor.constraint(equalTo: scrollView.contentLayoutGuide.trailingAnchor, constant: -InkSpacing.pageMargin),
            content.topAnchor.constraint(equalTo: scrollView.contentLayoutGuide.topAnchor, constant: InkSpacing.space3),
            content.bottomAnchor.constraint(equalTo: scrollView.contentLayoutGuide.bottomAnchor, constant: -InkSpacing.space16),
            content.widthAnchor.constraint(equalTo: scrollView.frameLayoutGuide.widthAnchor, constant: -2 * InkSpacing.pageMargin),
        ])
        scrollView.contentInsetAdjustmentBehavior = .automatic

        // MARK: Back

        // TODO(l10n): localize once the strings pass lands.
        let backButton = InkIconButton(symbol: "chevron.backward", accessibilityLabel: "Back") { [weak self] in
            self?.navigationController?.popViewController(animated: true)
        }
        let backRow = UIStackView(arrangedSubviews: [backButton, UIView()])
        backRow.axis = .horizontal
        content.addArrangedSubview(backRow)
        content.setCustomSpacing(18, after: backRow)

        // MARK: Cover block

        let cover = BookCoverView(title: book.title, author: book.author, seed: book.coverSeed)
        cover.widthAnchor.constraint(equalToConstant: 150).isActive = true

        let titleLabel = InkLabel()
        titleLabel.text = book.title
        titleLabel.font = InkFont.displaySmall
        titleLabel.textColor = InkColor.textDisplay
        titleLabel.textAlignment = .center
        titleLabel.numberOfLines = 0

        let authorLabel = InkLabel()
        authorLabel.text = book.author
        authorLabel.font = InkFont.labelRegular
        authorLabel.textColor = InkColor.textSecondary

        let progressBar = InkProgressBar(progress: book.progress)
        progressBar.widthAnchor.constraint(equalToConstant: 200).isActive = true

        let metaLabel = InkLabel()
        metaLabel.text = book.metaText
        metaLabel.font = InkFont.caption
        metaLabel.textColor = InkColor.textTertiary

        let readButton = InkButton("Keep reading", symbol: "book") { [weak self] in
            self?.openReader()
        }

        let coverBlock = UIStackView(arrangedSubviews: [cover, titleLabel, authorLabel, progressBar, metaLabel, readButton])
        coverBlock.axis = .vertical
        coverBlock.alignment = .center
        coverBlock.spacing = InkSpacing.space2
        coverBlock.setCustomSpacing(22, after: cover)
        coverBlock.setCustomSpacing(InkSpacing.space5, after: authorLabel)
        coverBlock.setCustomSpacing(18, after: metaLabel)
        content.addArrangedSubview(coverBlock)
        content.setCustomSpacing(InkSpacing.space10, after: coverBlock)

        // MARK: Contents

        let contentsTitle = InkLabel()
        contentsTitle.text = "Contents"
        contentsTitle.font = InkFont.sectionTitle
        contentsTitle.textColor = InkColor.textDisplay
        content.addArrangedSubview(contentsTitle)
        content.setCustomSpacing(InkSpacing.space2, after: contentsTitle)

        for (index, chapter) in PlaceholderLibrary.chapters.enumerated() {
            content.addArrangedSubview(chapterRow(chapter, index: index))
        }
    }

    private func chapterRow(_ chapter: PlaceholderChapter, index: Int) -> UIView {
        let isCurrent = index == PlaceholderLibrary.currentChapterIndex

        let numeral = InkLabel()
        numeral.text = chapter.numeral
        numeral.font = InkFont.caption
        numeral.textColor = isCurrent ? InkColor.accentText : InkColor.textTertiary
        numeral.widthAnchor.constraint(equalToConstant: 26).isActive = true

        let title = InkLabel()
        title.text = chapter.title
        title.font = InkFont.serif(16, weight: isCurrent ? .semibold : .regular, style: .body)
        title.textColor = isCurrent ? InkColor.accentText : InkColor.textDisplay

        let page = InkLabel()
        page.text = "p. \(chapter.page)"
        page.font = InkFont.caption
        page.textColor = InkColor.textTertiary

        let row = UIStackView(arrangedSubviews: [numeral, title, UIView(), page])
        row.axis = .horizontal
        row.alignment = .firstBaseline
        row.spacing = InkSpacing.space3
        row.isLayoutMarginsRelativeArrangement = true
        row.layoutMargins = UIEdgeInsets(top: 13, left: 2, bottom: 13, right: 2)

        let container = UIView()
        row.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(row)

        let separator = UIView()
        separator.backgroundColor = InkColor.borderHairline
        separator.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(separator)

        NSLayoutConstraint.activate([
            row.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            row.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            row.topAnchor.constraint(equalTo: container.topAnchor),
            row.bottomAnchor.constraint(equalTo: container.bottomAnchor),
            separator.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            separator.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            separator.bottomAnchor.constraint(equalTo: container.bottomAnchor),
            separator.heightAnchor.constraint(equalToConstant: 1 / traitCollection.displayScale),
        ])

        let tap = UITapGestureRecognizer(target: self, action: #selector(openReaderFromChapter))
        container.addGestureRecognizer(tap)
        container.isUserInteractionEnabled = true
        container.isAccessibilityElement = true
        container.accessibilityLabel = "Chapter \(chapter.numeral), \(chapter.title), page \(chapter.page)"
        container.accessibilityTraits = .button
        return container
    }

    @objc private func openReaderFromChapter() {
        // TODO(core): jump to the tapped chapter's position; the placeholder
        // reader always opens the current chapter.
        openReader()
    }

    private func openReader() {
        guard let navigationController else { return }
        // Reached from the reader's Contents button? Go back to the open
        // page instead of stacking a second reader.
        if navigationController.viewControllers.dropLast().last is ReaderViewController {
            navigationController.popViewController(animated: true)
            return
        }
        let reader = ReaderViewController(book: book)
        reader.hidesBottomBarWhenPushed = true
        navigationController.pushViewController(reader, animated: true)
    }
}
