import UIKit

/// Book cover (design-system `BookCover`): a 2:3 tile with the book
/// shadow. Until real cover art flows from the core, it renders the
/// design system's generated placeholder — a seeded ink-and-paper palette
/// with the title set in serif.
///
/// TODO(core): accept cover images from the Rust core's metadata extraction
/// and fall back to the generated cover only when a book ships without art.
final class BookCoverView: UIView {
    /// Seeded placeholder palettes from the design system (`BookCover.jsx`).
    private static let palettes: [(background: UInt32, foreground: UInt32)] = [
        (0x2E3440, 0xD8DEE9),
        (0x4A3B2A, 0xEFE7D9),
        (0x5C3A38, 0xF2E4DC),
        (0x33463C, 0xE4EDE6),
        (0x3A3A55, 0xE4E4F0),
        (0x6B5537, 0xF6EDD9),
    ]

    private let content = UIView()
    private let titleLabel = UILabel()
    private let authorLabel = UILabel()

    init(title: String, author: String, seed: Int) {
        super.init(frame: .zero)
        let palette = Self.palettes[abs(seed) % Self.palettes.count]

        installInkShadow(.book)

        content.backgroundColor = UIColor(ink: palette.background)
        content.layer.cornerRadius = InkRadius.xs
        content.layer.masksToBounds = true
        content.layer.borderWidth = 1
        content.layer.borderColor = UIColor.white.withAlphaComponent(0.06).cgColor
        content.translatesAutoresizingMaskIntoConstraints = false
        addSubview(content)

        titleLabel.text = title
        titleLabel.textColor = UIColor(ink: palette.foreground)
        titleLabel.numberOfLines = 3

        authorLabel.text = author
        authorLabel.textColor = UIColor(ink: palette.foreground, alpha: 0.8)
        authorLabel.numberOfLines = 1

        content.addSubview(titleLabel)
        content.addSubview(authorLabel)
        titleLabel.translatesAutoresizingMaskIntoConstraints = false
        authorLabel.translatesAutoresizingMaskIntoConstraints = false

        NSLayoutConstraint.activate([
            content.leadingAnchor.constraint(equalTo: leadingAnchor),
            content.trailingAnchor.constraint(equalTo: trailingAnchor),
            content.topAnchor.constraint(equalTo: topAnchor),
            content.bottomAnchor.constraint(equalTo: bottomAnchor),
            // Books stay rectangular: 2:3, like the design's covers.
            heightAnchor.constraint(equalTo: widthAnchor, multiplier: 1.5),

            titleLabel.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 10),
            titleLabel.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -10),
            titleLabel.topAnchor.constraint(equalTo: content.topAnchor, constant: 10),
            authorLabel.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 10),
            authorLabel.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -10),
            authorLabel.bottomAnchor.constraint(equalTo: content.bottomAnchor, constant: -10),
        ])
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    override func layoutSubviews() {
        super.layoutSubviews()
        // The placeholder cover's type scales with the tile, mirroring the
        // design system's `max(11, w * .13)` rules.
        let width = bounds.width
        guard width > 0 else { return }
        titleLabel.font = InkFont.serif(max(11, width * 0.13), weight: .medium, style: .caption1)
        authorLabel.font = InkFont.sans(max(8, width * 0.085), weight: .regular, style: .caption2)
    }
}
