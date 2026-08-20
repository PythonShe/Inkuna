import UIKit

/// The Customize panel's live specimen: a clipped slice of the reading
/// surface — theme colors, current text size — re-rendered with every
/// uncommitted style so the reader sees the typography before the page
/// behind the sheet has finished reflowing. Margins are deliberately not
/// applied; the card is about type, not geometry.
final class ReaderPreviewCard: UIView {
    var phrase: String {
        didSet { render() }
    }

    private let theme: ReadingTheme
    private let textSize: ReadingTextSize
    private var style: ReaderUserStyle
    private let label = InkLabel()

    init(theme: ReadingTheme, textSize: ReadingTextSize, phrase: String, style: ReaderUserStyle) {
        self.theme = theme
        self.textSize = textSize
        self.phrase = phrase
        self.style = style
        super.init(frame: .zero)

        backgroundColor = theme.background
        layer.cornerRadius = InkRadius.md
        clipsToBounds = true
        installInkShadow(.sm)

        label.numberOfLines = 0
        label.translatesAutoresizingMaskIntoConstraints = false
        addSubview(label)
        NSLayoutConstraint.activate([
            heightAnchor.constraint(lessThanOrEqualToConstant: 118),
            label.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 18),
            label.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -18),
            label.topAnchor.constraint(equalTo: topAnchor, constant: 16),
            label.bottomAnchor.constraint(lessThanOrEqualTo: bottomAnchor, constant: -16),
        ])

        isAccessibilityElement = true
        accessibilityTraits = .staticText
        render()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    func apply(_ style: ReaderUserStyle) {
        self.style = style
        render()
    }

    private func render() {
        let size = textSize.pointSize
        let paragraph = NSMutableParagraphStyle()
        // Exact CSS line-height semantics, not a multiple of the font's
        // own leading.
        paragraph.minimumLineHeight = size * style.lineSpacing
        paragraph.maximumLineHeight = size * style.lineSpacing
        paragraph.lineBreakMode = .byTruncatingTail

        let attributes: [NSAttributedString.Key: Any] = [
            .font: style.font.previewFont(size: size, bold: style.bold),
            .foregroundColor: theme.foreground,
            .paragraphStyle: paragraph,
            .kern: size * style.letterSpacing,
        ]
        let rendered = NSMutableAttributedString(string: phrase, attributes: attributes)
        // UIKit has no word-spacing attribute: extra kern on the space
        // separators only — which also makes it the correct no-op for
        // unspaced CJK text, exactly like the CSS property.
        if style.wordSpacing > 0 {
            for range in phrase.ranges(of: " ") {
                rendered.addAttribute(
                    .kern,
                    value: size * (style.letterSpacing + style.wordSpacing),
                    range: NSRange(range, in: phrase)
                )
            }
        }
        label.attributedText = rendered

        let previewName = String(localized: "a11y_preview", defaultValue: "Preview")
        accessibilityLabel = "\(previewName): \(phrase)"
    }
}
