import UIKit

/// In-reader contents: a native detent sheet with the book header and the
/// chapter list, the current chapter washed in accent.
///
/// TODO(core): chapters and jump targets come from the core's TOC; picking
/// a row will reposition the reader through Readium.
final class ContentsSheetViewController: UIViewController {
    var onSelectChapter: ((Int) -> Void)?

    private let book: PlaceholderBook
    private let pageInfoText: String

    init(book: PlaceholderBook, pageInfoText: String) {
        self.book = book
        self.pageInfoText = pageInfoText
        super.init(nibName: nil, bundle: nil)
        modalPresentationStyle = .pageSheet
        if let sheet = sheetPresentationController {
            sheet.detents = [.medium(), .large()]
            sheet.prefersGrabberVisible = true
            sheet.preferredCornerRadius = InkRadius.lg
        }
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = InkColor.bgSurface

        // MARK: Header

        let cover = BookCoverView(title: "", author: "", seed: book.coverSeed)
        NSLayoutConstraint.activate([
            cover.widthAnchor.constraint(equalToConstant: 34),
        ])

        let titleLabel = InkLabel()
        titleLabel.text = book.title
        titleLabel.font = InkFont.serif(15, weight: .medium, style: .subheadline)
        titleLabel.textColor = InkColor.textDisplay
        titleLabel.numberOfLines = 1

        let positionLabel = InkLabel()
        positionLabel.text = pageInfoText
        positionLabel.font = InkFont.caption
        positionLabel.textColor = InkColor.textTertiary

        let titleColumn = UIStackView(arrangedSubviews: [titleLabel, positionLabel])
        titleColumn.axis = .vertical
        titleColumn.spacing = 2

        let closeButton = InkCloseButton { [weak self] in self?.dismiss(animated: true) }

        let header = UIStackView(arrangedSubviews: [cover, titleColumn, closeButton])
        header.axis = .horizontal
        header.alignment = .center
        header.spacing = 13
        header.isLayoutMarginsRelativeArrangement = true
        header.layoutMargins = UIEdgeInsets(top: InkSpacing.space4, left: InkSpacing.space4, bottom: InkSpacing.space3, right: InkSpacing.space4)

        let hairline = UIView()
        hairline.backgroundColor = InkColor.borderHairline
        NSLayoutConstraint.activate([
            hairline.heightAnchor.constraint(equalToConstant: 1 / traitCollection.displayScale),
        ])

        // MARK: Chapter list

        let listStack = UIStackView()
        listStack.axis = .vertical
        for (index, chapter) in PlaceholderLibrary.chapters.enumerated() {
            listStack.addArrangedSubview(makeRow(chapter: chapter, index: index))
        }

        let scrollView = UIScrollView()
        scrollView.alwaysBounceVertical = true
        listStack.translatesAutoresizingMaskIntoConstraints = false
        scrollView.addSubview(listStack)

        let column = UIStackView(arrangedSubviews: [header, hairline, scrollView])
        column.axis = .vertical
        column.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(column)

        NSLayoutConstraint.activate([
            column.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            column.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            column.topAnchor.constraint(equalTo: view.topAnchor, constant: InkSpacing.space2),
            column.bottomAnchor.constraint(equalTo: view.bottomAnchor),
            listStack.leadingAnchor.constraint(equalTo: scrollView.contentLayoutGuide.leadingAnchor, constant: InkSpacing.space4),
            listStack.trailingAnchor.constraint(equalTo: scrollView.contentLayoutGuide.trailingAnchor, constant: -InkSpacing.space4),
            listStack.topAnchor.constraint(equalTo: scrollView.contentLayoutGuide.topAnchor, constant: 6),
            listStack.bottomAnchor.constraint(equalTo: scrollView.contentLayoutGuide.bottomAnchor, constant: -InkSpacing.space3),
            listStack.widthAnchor.constraint(equalTo: scrollView.frameLayoutGuide.widthAnchor, constant: -2 * InkSpacing.space4),
        ])
    }

    private func makeRow(chapter: PlaceholderChapter, index: Int) -> UIView {
        let isCurrent = index == PlaceholderLibrary.currentChapterIndex

        let numeral = InkLabel()
        numeral.text = chapter.numeral
        numeral.font = InkFont.caption
        numeral.textColor = isCurrent ? InkColor.accent : InkColor.textTertiary
        numeral.setContentHuggingPriority(.required, for: .horizontal)
        numeral.setContentCompressionResistancePriority(.required, for: .horizontal)
        NSLayoutConstraint.activate([numeral.widthAnchor.constraint(greaterThanOrEqualToConstant: 22)])

        let title = InkLabel()
        title.text = chapter.title
        title.font = InkFont.serif(16, weight: isCurrent ? .semibold : .regular, style: .body)
        title.textColor = isCurrent ? InkColor.accent : InkColor.textDisplay
        title.numberOfLines = 0
        // The title owns the leftover width; numeral and page stay snug.
        title.setContentHuggingPriority(.defaultLow, for: .horizontal)

        let page = InkLabel()
        page.text = "p. \(chapter.page)"
        page.font = InkFont.caption
        page.textColor = InkColor.textTertiary
        page.setContentHuggingPriority(.required, for: .horizontal)
        page.setContentCompressionResistancePriority(.required, for: .horizontal)

        let row = ChapterRowControl(isCurrent: isCurrent) { [weak self] in
            self?.onSelectChapter?(index)
            self?.dismiss(animated: true)
        }
        row.backgroundColor = isCurrent ? InkColor.accentSoft : .clear
        row.layer.cornerRadius = InkRadius.sm

        let content = UIStackView(arrangedSubviews: [numeral, title, page])
        content.axis = .horizontal
        content.alignment = .firstBaseline
        content.spacing = 12
        content.isUserInteractionEnabled = false
        content.translatesAutoresizingMaskIntoConstraints = false
        row.addSubview(content)
        NSLayoutConstraint.activate([
            content.leadingAnchor.constraint(equalTo: row.leadingAnchor, constant: 10),
            content.trailingAnchor.constraint(equalTo: row.trailingAnchor, constant: -10),
            content.topAnchor.constraint(equalTo: row.topAnchor, constant: 13),
            content.bottomAnchor.constraint(equalTo: row.bottomAnchor, constant: -13),
        ])

        row.isAccessibilityElement = true
        row.accessibilityLabel = "\(chapter.numeral). \(chapter.title), page \(chapter.page)"
        row.accessibilityTraits = isCurrent ? [.button, .selected] : .button
        return row
    }

    /// Tappable chapter row with a soft pressed wash. The current chapter
    /// already rests on the accent wash, so it dims instead.
    private final class ChapterRowControl: UIControl {
        private let isCurrent: Bool
        private var restingColor: UIColor?

        init(isCurrent: Bool, handler: @escaping @MainActor () -> Void) {
            self.isCurrent = isCurrent
            super.init(frame: .zero)
            addAction(UIAction { _ in handler() }, for: .touchUpInside)
        }

        @available(*, unavailable)
        required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

        override var isHighlighted: Bool {
            didSet {
                guard isHighlighted != oldValue else { return }
                if isCurrent {
                    alpha = isHighlighted ? 0.7 : 1
                } else if isHighlighted {
                    restingColor = backgroundColor
                    backgroundColor = InkColor.accentSoft
                } else {
                    backgroundColor = restingColor
                }
            }
        }
    }
}
