import UIKit

/// Library/search list row (design-system `BookListRow`): a small cover,
/// title and author, a short progress bar with percentage, and a
/// cloud badge when the book still lives remotely.
final class BookListRowView: UIView {
    private let coverContainer = UIView()
    private let titleLabel = UILabel()
    private let authorLabel = UILabel()
    private let progressBar = InkProgressBar()
    private let percentLabel = UILabel()
    private let cloudBadge = UIImageView(
        image: UIImage(
            systemName: "icloud.and.arrow.down",
            withConfiguration: UIImage.SymbolConfiguration(pointSize: 16, weight: .regular)
        )
    )

    override init(frame: CGRect) {
        super.init(frame: frame)

        titleLabel.font = InkFont.serif(17, weight: .medium, style: .body)
        titleLabel.textColor = InkColor.textDisplay
        titleLabel.numberOfLines = 1

        authorLabel.font = InkFont.labelRegular
        authorLabel.textColor = InkColor.textSecondary
        authorLabel.numberOfLines = 1

        percentLabel.font = InkFont.caption
        percentLabel.textColor = InkColor.textTertiary

        cloudBadge.tintColor = InkColor.textTertiary
        cloudBadge.setContentHuggingPriority(.required, for: .horizontal)
        cloudBadge.setContentCompressionResistancePriority(.required, for: .horizontal)

        let progressRow = UIStackView(arrangedSubviews: [progressBar, percentLabel, UIView()])
        progressRow.axis = .horizontal
        progressRow.spacing = InkSpacing.space2
        progressRow.alignment = .center

        let textStack = UIStackView(arrangedSubviews: [titleLabel, authorLabel, progressRow])
        textStack.axis = .vertical
        textStack.spacing = 2
        textStack.setCustomSpacing(InkSpacing.space2, after: authorLabel)

        let row = UIStackView(arrangedSubviews: [coverContainer, textStack, cloudBadge])
        row.axis = .horizontal
        row.spacing = 14
        row.alignment = .center
        row.translatesAutoresizingMaskIntoConstraints = false
        addSubview(row)

        NSLayoutConstraint.activate([
            row.leadingAnchor.constraint(equalTo: leadingAnchor),
            row.trailingAnchor.constraint(equalTo: trailingAnchor),
            row.topAnchor.constraint(equalTo: topAnchor, constant: InkSpacing.space3),
            row.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -InkSpacing.space3),
            coverContainer.widthAnchor.constraint(equalToConstant: 52),
            progressBar.widthAnchor.constraint(equalToConstant: 72),
        ])
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    func configure(title: String, author: String, progress: CGFloat?, seed: Int, downloaded: Bool) {
        titleLabel.text = title
        authorLabel.text = author
        cloudBadge.isHidden = downloaded

        coverContainer.subviews.forEach { $0.removeFromSuperview() }
        let cover = BookCoverView(title: title, author: author, seed: seed)
        cover.translatesAutoresizingMaskIntoConstraints = false
        coverContainer.addSubview(cover)
        NSLayoutConstraint.activate([
            cover.leadingAnchor.constraint(equalTo: coverContainer.leadingAnchor),
            cover.trailingAnchor.constraint(equalTo: coverContainer.trailingAnchor),
            cover.topAnchor.constraint(equalTo: coverContainer.topAnchor),
            cover.bottomAnchor.constraint(equalTo: coverContainer.bottomAnchor),
        ])

        if let progress {
            progressBar.isHidden = false
            percentLabel.isHidden = false
            progressBar.setProgress(progress, animated: false)
            percentLabel.text = "\(Int((progress * 100).rounded()))%"
        } else {
            progressBar.isHidden = true
            percentLabel.isHidden = true
        }
    }
}
