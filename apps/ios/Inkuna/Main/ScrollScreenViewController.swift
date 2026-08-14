import UIKit

/// Shared scaffold for the tab screens: a vertical content stack in a
/// scroll view, page-margin gutters, and bottom clearance for the
/// floating tab bar.
class ScrollScreenViewController: UIViewController {
    let scrollView = UIScrollView()
    let contentStack = UIStackView()

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = InkColor.bgApp

        scrollView.alwaysBounceVertical = true
        scrollView.showsVerticalScrollIndicator = false
        scrollView.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(scrollView)

        contentStack.axis = .vertical
        contentStack.translatesAutoresizingMaskIntoConstraints = false
        scrollView.addSubview(contentStack)

        NSLayoutConstraint.activate([
            scrollView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            scrollView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            scrollView.topAnchor.constraint(equalTo: view.topAnchor),
            scrollView.bottomAnchor.constraint(equalTo: view.bottomAnchor),
            contentStack.leadingAnchor.constraint(equalTo: scrollView.contentLayoutGuide.leadingAnchor, constant: InkSpacing.pageMargin),
            contentStack.trailingAnchor.constraint(equalTo: scrollView.contentLayoutGuide.trailingAnchor, constant: -InkSpacing.pageMargin),
            contentStack.topAnchor.constraint(equalTo: scrollView.contentLayoutGuide.topAnchor, constant: InkSpacing.space6),
            contentStack.bottomAnchor.constraint(equalTo: scrollView.contentLayoutGuide.bottomAnchor, constant: -InkSpacing.space8),
            contentStack.widthAnchor.constraint(equalTo: scrollView.frameLayoutGuide.widthAnchor, constant: -2 * InkSpacing.pageMargin),
        ])
    }

    // MARK: Shared building blocks

    /// Screen-level display title (`--type-display`).
    func displayTitle(_ text: String) -> UILabel {
        let label = UILabel()
        label.text = text
        label.font = InkFont.display
        label.textColor = InkColor.textDisplay
        label.numberOfLines = 0
        return label
    }

    /// In-screen section title (1.4rem serif).
    func sectionTitle(_ text: String) -> UILabel {
        let label = UILabel()
        label.text = text
        label.font = InkFont.sectionTitle
        label.textColor = InkColor.textDisplay
        return label
    }

    /// Uppercase tracked eyebrow (`--type-caption` + `--tracking-caps`).
    func eyebrowLabel(_ text: String) -> UILabel {
        let label = UILabel()
        label.attributedText = InkFont.eyebrow(text)
        return label
    }

    /// Centered quiet empty-state message.
    func emptyStateLabel(_ text: String) -> UILabel {
        let label = UILabel()
        label.text = text
        label.font = InkFont.reading()
        label.textColor = InkColor.textTertiary
        label.textAlignment = .center
        label.numberOfLines = 0
        return label
    }

    /// Full-page destinations slide the native tab bar away.
    func openBook(_ book: PlaceholderBook) {
        let detail = BookDetailViewController(book: book)
        detail.hidesBottomBarWhenPushed = true
        navigationController?.pushViewController(detail, animated: true)
    }

    func openReader(_ book: PlaceholderBook) {
        let reader = ReaderViewController(book: book)
        reader.hidesBottomBarWhenPushed = true
        navigationController?.pushViewController(reader, animated: true)
    }
}
