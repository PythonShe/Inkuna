import UIKit

/// In-reader contents: a native detent sheet with the book header and the
/// core's flattened TOC, the current chapter washed in accent. Picking a row
/// hands the chapter back to the reader, which jumps the navigator.
///
/// The list is a diffable collection view, not a built-up stack: a long
/// publication's TOC can run to hundreds of entries, and materializing a
/// constrained row per chapter froze the sheet's presentation for seconds.
final class ContentsSheetViewController: UIViewController {
    /// One chapter of the core TOC, joined with its core-derived synthetic
    /// start position (nil when no sparse chapter range exists) and whether
    /// the reader is currently inside it.
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
    private var dataSource: UICollectionViewDiffableDataSource<Int, Int>?
    private weak var listView: UICollectionView?

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

        let listView = makeListView()
        let column = UIStackView(arrangedSubviews: [header, hairline, listView])
        column.axis = .vertical
        column.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(column)

        NSLayoutConstraint.activate([
            column.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            column.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            column.topAnchor.constraint(equalTo: view.topAnchor, constant: InkSpacing.space2),
            column.bottomAnchor.constraint(equalTo: view.bottomAnchor),
        ])

        if rows.isEmpty {
            let empty = InkLabel()
            empty.text = String(localized: "detail_no_contents", defaultValue: "This book lists no contents.")
            empty.font = InkFont.reading()
            empty.textColor = InkColor.textTertiary
            empty.textAlignment = .center
            empty.numberOfLines = 0
            empty.translatesAutoresizingMaskIntoConstraints = false
            let wrapper = UIView()
            wrapper.addSubview(empty)
            NSLayoutConstraint.activate([
                empty.leadingAnchor.constraint(equalTo: wrapper.leadingAnchor, constant: InkSpacing.space4),
                empty.trailingAnchor.constraint(equalTo: wrapper.trailingAnchor, constant: -InkSpacing.space4),
                empty.topAnchor.constraint(equalTo: wrapper.topAnchor, constant: InkSpacing.space12),
            ])
            listView.backgroundView = wrapper
        }

        var snapshot = NSDiffableDataSourceSnapshot<Int, Int>()
        snapshot.appendSections([0])
        snapshot.appendItems(Array(rows.indices))
        dataSource?.apply(snapshot, animatingDifferences: false)
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        revealCurrentChapterIfNeeded()
    }

    /// One initial scroll that lands the reader's chapter in view — with
    /// hundreds of entries the current one is rarely near the top.
    private var didRevealCurrent = false
    private func revealCurrentChapterIfNeeded() {
        guard !didRevealCurrent, let listView, listView.bounds.height > 0 else { return }
        didRevealCurrent = true
        guard let current = rows.firstIndex(where: { $0.isCurrent }) else { return }
        listView.layoutIfNeeded()
        listView.scrollToItem(
            at: IndexPath(item: current, section: 0),
            at: .centeredVertically,
            animated: false
        )
    }

    private func makeListView() -> UICollectionView {
        var configuration = UICollectionLayoutListConfiguration(appearance: .plain)
        configuration.backgroundColor = .clear
        configuration.showsSeparators = false
        let layout = UICollectionViewCompositionalLayout.list(using: configuration)
        let listView = UICollectionView(frame: .zero, collectionViewLayout: layout)
        listView.backgroundColor = .clear
        listView.alwaysBounceVertical = true
        listView.delegate = self
        listView.contentInset = UIEdgeInsets(top: 6, left: 0, bottom: InkSpacing.space3, right: 0)
        self.listView = listView

        let registration = UICollectionView.CellRegistration<ChapterRowCell, Int> { [weak self] cell, _, index in
            guard let self, self.rows.indices.contains(index) else { return }
            cell.apply(self.rows[index])
        }
        dataSource = UICollectionViewDiffableDataSource<Int, Int>(collectionView: listView) { collectionView, indexPath, index in
            collectionView.dequeueConfiguredReusableCell(using: registration, for: indexPath, item: index)
        }
        return listView
    }

    /// Chapter row with a soft pressed wash. The current chapter already
    /// rests on the accent wash, so it dims instead.
    private final class ChapterRowCell: UICollectionViewCell {
        private let numeral = InkLabel()
        private let title = InkLabel()
        private let position = InkLabel()
        private let content = UIStackView()
        private var leadingInset: NSLayoutConstraint?
        private var isCurrent = false

        override init(frame: CGRect) {
            super.init(frame: frame)
            numeral.font = InkFont.caption
            numeral.setContentHuggingPriority(.required, for: .horizontal)
            numeral.setContentCompressionResistancePriority(.required, for: .horizontal)

            title.numberOfLines = 0
            // The title owns the leftover width; numeral and position stay snug.
            title.setContentHuggingPriority(.defaultLow, for: .horizontal)

            position.font = InkFont.caption
            position.textColor = InkColor.textTertiary
            position.setContentHuggingPriority(.required, for: .horizontal)
            position.setContentCompressionResistancePriority(.required, for: .horizontal)

            [numeral, title, position].forEach { content.addArrangedSubview($0) }
            content.axis = .horizontal
            content.alignment = .firstBaseline
            content.spacing = 12
            content.isUserInteractionEnabled = false
            content.translatesAutoresizingMaskIntoConstraints = false
            contentView.addSubview(content)
            contentView.layer.cornerRadius = InkRadius.sm
            let leading = content.leadingAnchor.constraint(equalTo: contentView.leadingAnchor, constant: InkSpacing.space4 + 10)
            leadingInset = leading
            NSLayoutConstraint.activate([
                numeral.widthAnchor.constraint(greaterThanOrEqualToConstant: 22),
                leading,
                content.trailingAnchor.constraint(equalTo: contentView.trailingAnchor, constant: -(InkSpacing.space4 + 10)),
                content.topAnchor.constraint(equalTo: contentView.topAnchor, constant: 13),
                content.bottomAnchor.constraint(equalTo: contentView.bottomAnchor, constant: -13),
            ])
        }

        @available(*, unavailable)
        required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

        func apply(_ row: Row) {
            let chapter = row.chapter
            isCurrent = row.isCurrent
            numeral.text = "\(chapter.idx + 1)"
            numeral.textColor = isCurrent ? InkColor.accentText : InkColor.textTertiary
            title.text = chapter.title
            title.font = InkFont.serif(16, weight: isCurrent ? .semibold : .regular, style: .body)
            title.textColor = isCurrent ? InkColor.accentText : InkColor.textDisplay
            if let page = row.position {
                let format = NSLocalizedString("reader_chapter_page", comment: "")
                position.text = String.localizedStringWithFormat(format, Int64(page))
                position.isHidden = false
            } else {
                // Honest synthetic positions only: the column is absent until
                // the navigator has computed positions for this book.
                position.text = nil
                position.isHidden = true
            }
            // Nested TOC entries step in with their depth.
            leadingInset?.constant = InkSpacing.space4 + 10 + CGFloat(min(chapter.depth, 4)) * 14
            contentView.backgroundColor = isCurrent ? InkColor.accentSoft : .clear
            contentView.alpha = 1

            isAccessibilityElement = true
            if let page = row.position {
                let format = NSLocalizedString("a11y_chapter_row", comment: "")
                accessibilityLabel = String.localizedStringWithFormat(format, "\(chapter.idx + 1)", chapter.title, Int64(page))
            } else {
                let format = NSLocalizedString("a11y_chapter_row_no_page", comment: "")
                accessibilityLabel = String.localizedStringWithFormat(format, "\(chapter.idx + 1)", chapter.title)
            }
            accessibilityTraits = isCurrent ? [.button, .selected] : .button
        }

        override var isHighlighted: Bool {
            didSet {
                guard isHighlighted != oldValue else { return }
                if isCurrent {
                    contentView.alpha = isHighlighted ? 0.7 : 1
                } else {
                    contentView.backgroundColor = isHighlighted ? InkColor.accentSoft : .clear
                }
            }
        }
    }
}

extension ContentsSheetViewController: UICollectionViewDelegate {
    func collectionView(_ collectionView: UICollectionView, didSelectItemAt indexPath: IndexPath) {
        collectionView.deselectItem(at: indexPath, animated: false)
        guard rows.indices.contains(indexPath.item) else { return }
        onSelectChapter?(rows[indexPath.item].chapter)
        dismiss(animated: true)
    }
}
