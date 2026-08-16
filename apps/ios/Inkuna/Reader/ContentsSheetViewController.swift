import UIKit

/// In-reader contents: a native detent sheet with the book header and the
/// core's flattened TOC, the current chapter washed in accent. Picking a row
/// hands the chapter back to the reader, which jumps the navigator.
final class ContentsSheetViewController: UIViewController {
    /// One chapter of the core TOC, joined with what the reader knows from
    /// Readium: the synthetic position its resource starts at (nil until
    /// positions are computed or when the href matches no resource) and
    /// whether the reader is currently inside it.
    struct Row {
        let chapter: Chapter
        let position: Int?
        var isCurrent: Bool
    }

    var onSelectChapter: ((Chapter) -> Void)?

    private let bookTitle: String
    private let coverSeed: Int
    private let coverPath: String?
    private let rows: [Row]
    private let pageInfoText: String

    init(bookTitle: String, coverSeed: Int, coverPath: String?, rows: [Row], pageInfoText: String) {
        self.bookTitle = bookTitle
        self.coverSeed = coverSeed
        self.coverPath = coverPath
        self.rows = rows
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

        let cover = BookCoverView(title: "", author: "", seed: coverSeed, coverPath: coverPath)
        NSLayoutConstraint.activate([
            cover.widthAnchor.constraint(equalToConstant: 34),
        ])

        let titleLabel = InkLabel()
        titleLabel.text = bookTitle
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
        if rows.isEmpty {
            // TODO(l10n): localize once the strings pass lands.
            let empty = InkLabel()
            empty.text = "This book lists no contents."
            empty.font = InkFont.reading()
            empty.textColor = InkColor.textTertiary
            empty.textAlignment = .center
            empty.numberOfLines = 0
            let wrapper = UIStackView(arrangedSubviews: [empty])
            wrapper.axis = .vertical
            wrapper.isLayoutMarginsRelativeArrangement = true
            wrapper.layoutMargins = UIEdgeInsets(top: InkSpacing.space12, left: 0, bottom: InkSpacing.space12, right: 0)
            listStack.addArrangedSubview(wrapper)
        } else {
            for row in rows {
                listStack.addArrangedSubview(makeRow(row))
            }
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

    private func makeRow(_ rowModel: Row) -> UIView {
        let chapter = rowModel.chapter
        let isCurrent = rowModel.isCurrent

        let numeral = InkLabel()
        numeral.text = "\(chapter.idx + 1)"
        numeral.font = InkFont.caption
        numeral.textColor = isCurrent ? InkColor.accentText : InkColor.textTertiary
        numeral.setContentHuggingPriority(.required, for: .horizontal)
        numeral.setContentCompressionResistancePriority(.required, for: .horizontal)
        NSLayoutConstraint.activate([numeral.widthAnchor.constraint(greaterThanOrEqualToConstant: 22)])

        let title = InkLabel()
        title.text = chapter.title
        title.font = InkFont.serif(16, weight: isCurrent ? .semibold : .regular, style: .body)
        title.textColor = isCurrent ? InkColor.accentText : InkColor.textDisplay
        title.numberOfLines = 0
        // The title owns the leftover width; numeral and position stay snug.
        title.setContentHuggingPriority(.defaultLow, for: .horizontal)

        let content = UIStackView(arrangedSubviews: [numeral, title])
        // Honest synthetic positions only: the column is absent until the
        // navigator has computed positions for this book.
        if let position = rowModel.position {
            let positionLabel = InkLabel()
            // TODO(l10n): localize once the strings pass lands.
            positionLabel.text = "p. \(position)"
            positionLabel.font = InkFont.caption
            positionLabel.textColor = InkColor.textTertiary
            positionLabel.setContentHuggingPriority(.required, for: .horizontal)
            positionLabel.setContentCompressionResistancePriority(.required, for: .horizontal)
            content.addArrangedSubview(positionLabel)
        }

        let row = ChapterRowControl(isCurrent: isCurrent) { [weak self] in
            self?.onSelectChapter?(chapter)
            self?.dismiss(animated: true)
        }
        row.backgroundColor = isCurrent ? InkColor.accentSoft : .clear
        row.layer.cornerRadius = InkRadius.sm

        content.axis = .horizontal
        content.alignment = .firstBaseline
        content.spacing = 12
        content.isUserInteractionEnabled = false
        content.translatesAutoresizingMaskIntoConstraints = false
        row.addSubview(content)
        // Nested TOC entries step in with their depth.
        let leadingInset = 10 + CGFloat(min(chapter.depth, 4)) * 14
        NSLayoutConstraint.activate([
            content.leadingAnchor.constraint(equalTo: row.leadingAnchor, constant: leadingInset),
            content.trailingAnchor.constraint(equalTo: row.trailingAnchor, constant: -10),
            content.topAnchor.constraint(equalTo: row.topAnchor, constant: 13),
            content.bottomAnchor.constraint(equalTo: row.bottomAnchor, constant: -13),
        ])

        row.isAccessibilityElement = true
        // TODO(l10n): localize once the strings pass lands.
        var accessibilityLabel = "\(chapter.idx + 1). \(chapter.title)"
        if let position = rowModel.position {
            accessibilityLabel += ", position \(position)"
        }
        row.accessibilityLabel = accessibilityLabel
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
