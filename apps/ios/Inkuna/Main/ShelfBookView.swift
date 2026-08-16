import UIKit

/// A standing book on a horizontal shelf: cover with title and author
/// beneath (the "On the nightstand" / "New this week" pattern).
final class ShelfBookView: UIControl {
    let publication: Publication

    var onOpen: ((Publication) -> Void)?

    init(publication: Publication, width: CGFloat = 104) {
        self.publication = publication
        super.init(frame: .zero)

        // TODO(l10n): localize once the strings pass lands.
        let author = publication.displayAuthors(unknownAuthor: "Unknown author")
        let cover = BookCoverView(
            title: publication.title,
            author: author,
            seed: BookCoverView.coverSeed(for: publication.id),
            coverPath: publication.coverPath
        )
        cover.isUserInteractionEnabled = false

        let titleLabel = InkLabel()
        titleLabel.text = publication.title
        titleLabel.font = InkFont.serif(14, weight: .medium, style: .footnote)
        titleLabel.textColor = InkColor.textDisplay
        titleLabel.numberOfLines = 2

        let authorLabel = InkLabel()
        authorLabel.text = author
        authorLabel.font = InkFont.caption
        authorLabel.textColor = InkColor.textTertiary
        authorLabel.numberOfLines = 1

        let stack = UIStackView(arrangedSubviews: [cover, titleLabel, authorLabel])
        stack.axis = .vertical
        stack.alignment = .leading
        stack.spacing = 2
        stack.setCustomSpacing(9, after: cover)
        stack.isUserInteractionEnabled = false
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)

        NSLayoutConstraint.activate([
            widthAnchor.constraint(equalToConstant: width),
            cover.widthAnchor.constraint(equalToConstant: width),
            stack.leadingAnchor.constraint(equalTo: leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor),
            stack.topAnchor.constraint(equalTo: topAnchor),
            stack.bottomAnchor.constraint(lessThanOrEqualTo: bottomAnchor),
        ])

        isAccessibilityElement = true
        accessibilityLabel = "\(publication.title), \(author)"
        accessibilityTraits = .button

        // .touchUpInside, not .primaryActionTriggered: plain UIControl
        // subclasses never emit the latter.
        addAction(UIAction { [weak self] _ in
            guard let self else { return }
            self.onOpen?(self.publication)
        }, for: .touchUpInside)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    override var isHighlighted: Bool {
        didSet {
            guard isHighlighted != oldValue else { return }
            let pressed = isHighlighted
            InkMotion.runQuiet(duration: InkMotion.fast) {
                self.alpha = pressed ? 0.7 : 1
            }
        }
    }
}
