import UIKit

/// The sheet shown when an import has something to say beyond "done":
/// a duplicate to name, a file that didn't make it, a run the user stopped.
///
/// Every row names the thing it is talking about — the book's own title
/// when the core got far enough to read one, the file's name when it did
/// not — and no row truncates, because a title the reader cannot finish
/// reading is not an explanation.
final class ImportSummaryViewController: UIViewController {
    private enum Section {
        case main
    }

    private let report: ImportReport
    private var collectionView: UICollectionView!
    private var dataSource: UICollectionViewDiffableDataSource<Section, Int>!

    init(report: ImportReport) {
        self.report = report
        super.init(nibName: nil, bundle: nil)
        modalPresentationStyle = .pageSheet
        if let sheet = sheetPresentationController {
            sheet.detents = [.medium(), .large()]
            sheet.prefersGrabberVisible = true
            sheet.preferredCornerRadius = InkRadius.xl
        }
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = InkColor.bgApp

        let header = buildHeader()
        let footer = buildFooter()
        buildCollectionView()

        for subview in [header, collectionView, footer] as [UIView] {
            subview.translatesAutoresizingMaskIntoConstraints = false
            view.addSubview(subview)
        }

        NSLayoutConstraint.activate([
            header.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: InkSpacing.pageMargin),
            header.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -InkSpacing.pageMargin),
            header.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: InkSpacing.space6),

            collectionView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            collectionView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            collectionView.topAnchor.constraint(equalTo: header.bottomAnchor, constant: InkSpacing.space4),
            collectionView.bottomAnchor.constraint(equalTo: footer.topAnchor, constant: -InkSpacing.space2),

            footer.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: InkSpacing.pageMargin),
            footer.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -InkSpacing.pageMargin),
            footer.bottomAnchor.constraint(
                equalTo: view.safeAreaLayoutGuide.bottomAnchor,
                constant: -InkSpacing.space4
            ),
        ])

        applySnapshot()
    }

    // MARK: Chrome

    private func buildHeader() -> UIView {
        let title = InkLabel()
        title.text = ImportCopy.summaryTitle(report)
        title.font = InkFont.sectionTitle
        title.textColor = InkColor.textDisplay
        title.numberOfLines = 0

        let stack = UIStackView(arrangedSubviews: [title])
        stack.axis = .vertical
        stack.spacing = InkSpacing.space1

        if let subtitle = ImportCopy.summarySubtitle(report) {
            let label = InkLabel()
            label.text = subtitle
            label.font = InkFont.labelRegular
            label.textColor = InkColor.textSecondary
            label.numberOfLines = 0
            stack.addArrangedSubview(label)
        }
        return stack
    }

    private func buildFooter() -> UIView {
        let done = InkButton(ImportCopy.done, variant: .primary, size: .medium) { [weak self] in
            self?.dismiss(animated: true)
        }
        // Centered in a container, not between stack spacers, so the pill
        // keeps its natural width instead of being compressed to its title.
        let row = UIView()
        done.translatesAutoresizingMaskIntoConstraints = false
        row.addSubview(done)
        NSLayoutConstraint.activate([
            done.centerXAnchor.constraint(equalTo: row.centerXAnchor),
            done.topAnchor.constraint(equalTo: row.topAnchor),
            done.bottomAnchor.constraint(equalTo: row.bottomAnchor),
            done.leadingAnchor.constraint(greaterThanOrEqualTo: row.leadingAnchor),
        ])
        return row
    }

    // MARK: List

    private func buildCollectionView() {
        var configuration = UICollectionLayoutListConfiguration(appearance: .plain)
        configuration.backgroundColor = .clear
        configuration.showsSeparators = true
        let layout = UICollectionViewCompositionalLayout.list(using: configuration)

        collectionView = UICollectionView(frame: .zero, collectionViewLayout: layout)
        collectionView.backgroundColor = .clear
        collectionView.alwaysBounceVertical = true
        collectionView.allowsSelection = false

        let cell = UICollectionView.CellRegistration<UICollectionViewListCell, Int> { [report] cell, _, index in
            guard report.items.indices.contains(index) else { return }
            let row = Row(report.items[index])

            var content = UIListContentConfiguration.subtitleCell()
            content.image = UIImage(
                systemName: row.symbol,
                withConfiguration: UIImage.SymbolConfiguration(pointSize: 17, weight: .regular)
            )
            content.imageProperties.tintColor = row.tint
            content.imageToTextPadding = InkSpacing.space3
            content.text = row.title
            content.textProperties.font = InkFont.ui
            content.textProperties.color = InkColor.textDisplay
            content.textProperties.numberOfLines = 0
            content.secondaryText = row.detail
            content.secondaryTextProperties.font = InkFont.labelRegular
            content.secondaryTextProperties.color = row.detailColor
            content.secondaryTextProperties.numberOfLines = 0
            content.textToSecondaryTextVerticalPadding = 2
            content.directionalLayoutMargins = NSDirectionalEdgeInsets(
                top: InkSpacing.space3,
                leading: InkSpacing.pageMargin,
                bottom: InkSpacing.space3,
                trailing: InkSpacing.pageMargin
            )
            cell.contentConfiguration = content

            var background = UIBackgroundConfiguration.clear()
            background.backgroundColor = .clear
            cell.backgroundConfiguration = background
            cell.accessibilityLabel = "\(row.title). \(row.detail)"
        }

        dataSource = UICollectionViewDiffableDataSource(collectionView: collectionView) { view, indexPath, index in
            view.dequeueConfiguredReusableCell(using: cell, for: indexPath, item: index)
        }
    }

    private func applySnapshot() {
        var snapshot = NSDiffableDataSourceSnapshot<Section, Int>()
        snapshot.appendSections([.main])
        snapshot.appendItems(Array(report.items.indices))
        dataSource.apply(snapshot, animatingDifferences: false)
    }

    // MARK: Row model

    /// One outcome, flattened into what a row draws.
    private struct Row {
        let symbol: String
        let tint: UIColor
        let title: String
        let detail: String
        let detailColor: UIColor

        init(_ outcome: ImportItemOutcome) {
            switch outcome {
            case .imported(let publication, _):
                symbol = "checkmark.circle.fill"
                tint = InkColor.positive
                title = publication.title
                detail = publication.authors.isEmpty
                    ? ImportCopy.statusAdded
                    : publication.authors.joined(separator: ", ")
                detailColor = InkColor.textSecondary
            case .duplicate(let publication, _):
                symbol = "books.vertical.fill"
                tint = InkColor.accentText
                title = publication.title
                detail = ImportCopy.statusDuplicate
                detailColor = InkColor.textSecondary
            case .failed(let failure):
                symbol = "exclamationmark.triangle.fill"
                tint = InkColor.danger
                title = failure.fileName
                detail = ImportCopy.explanation(failure.reason)
                detailColor = InkColor.textSecondary
            }
        }
    }
}
