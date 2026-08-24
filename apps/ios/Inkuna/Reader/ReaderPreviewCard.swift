import CoreText
import UIKit

/// The Customize panel's live specimen: a clipped slice of the reading
/// surface — theme colors, current text size — re-rendered with every
/// Customize change so the reader sees the typography before the page
/// behind the sheet has finished reflowing. Margins are deliberately not
/// applied; the card is about type, not geometry.
final class ReaderPreviewCard: UIView {
    var phrase: String {
        didSet { render() }
    }

    /// The live session's face for the Publisher roster entry, sampled
    /// from the page being read (see the reader's provider). `nil` —
    /// no session, page not laid out yet — falls back to the serif
    /// stand-in of `ReadingFont.previewFont`.
    var publisherFontProvider: ((CGFloat) -> CTFont?)?

    private let theme: ReadingTheme
    private let textSize: ReadingTextSize
    private let label = InkLabel()

    init(theme: ReadingTheme, textSize: ReadingTextSize, phrase: String) {
        self.theme = theme
        self.textSize = textSize
        self.phrase = phrase
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

    func apply() { render() }

    private func render() {
        let settings = AppSettings.shared
        let size = textSize.pointSize
        let paragraph = NSMutableParagraphStyle()
        // Exact CSS line-height semantics, not a multiple of the font's
        // own leading.
        paragraph.minimumLineHeight = size * settings.lineSpacing
        paragraph.maximumLineHeight = size * settings.lineSpacing
        paragraph.lineBreakMode = .byTruncatingTail

        // CTFont is toll-free bridged to UIFont, so a sampled publisher
        // face drops straight into the attribute dictionary.
        let font: Any = if settings.readingFont == .publisher,
            let publisher = publisherFontProvider?(size) {
            publisher
        } else {
            settings.readingFont.previewFont(size: size, bold: settings.readingBold)
        }
        let attributes: [NSAttributedString.Key: Any] = [
            .font: font,
            .foregroundColor: theme.foreground,
            .paragraphStyle: paragraph,
            .kern: size * settings.letterSpacing,
        ]
        let rendered = NSMutableAttributedString(string: phrase, attributes: attributes)
        // UIKit has no word-spacing attribute: extra kern on the space
        // separators only — which also makes it the correct no-op for
        // unspaced CJK text, exactly like the CSS property.
        if settings.wordSpacing > 0 {
            for range in phrase.ranges(of: " ") {
                rendered.addAttribute(
                    .kern,
                    value: size * (settings.letterSpacing + settings.wordSpacing),
                    range: NSRange(range, in: phrase)
                )
            }
        }
        label.attributedText = rendered

        let previewName = String(localized: "a11y_preview", defaultValue: "Preview")
        accessibilityLabel = "\(previewName): \(phrase)"
    }
}
