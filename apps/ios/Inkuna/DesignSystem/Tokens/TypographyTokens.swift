import UIKit

/// Typography tokens ported from the Inkuna design system
/// (`tokens/typography.css`).
///
/// The design system pairs a literary serif (Newsreader) with a plain
/// grotesque (Instrument Sans). On iOS the native equivalents are New York
/// (system serif design) and SF — both ship with the OS, support Dynamic
/// Type, and carry full CJK fallbacks, which is a core product goal.
enum InkFont {
    // MARK: Roles (CSS `--type-*`)

    /// `--type-display` — 40pt light serif, hero/screen titles.
    static var display: UIFont { serif(40, weight: .light, style: .largeTitle) }
    /// `--type-title` — 28pt serif, section titles.
    static var title: UIFont { serif(28, weight: .regular, style: .title1) }
    /// In-screen section titles (`--type-title` at 1.4rem).
    static var sectionTitle: UIFont { serif(22, weight: .regular, style: .title2) }
    /// `--type-heading` — 20pt medium serif, card/book titles.
    static var heading: UIFont { serif(20, weight: .medium, style: .title3) }
    /// Book-detail / stat numerals — 30pt light serif.
    static var displaySmall: UIFont { serif(30, weight: .light, style: .title1) }
    /// `--type-reading` — serif book body at the reader's selected size.
    static func reading(_ size: ReadingTextSize = .medium) -> UIFont {
        serif(size.pointSize, weight: .regular, style: .body)
    }
    /// `--type-ui` — 15pt sans medium, UI labels.
    static var ui: UIFont { sans(15, weight: .medium, style: .callout) }
    /// `--type-label` — 13pt sans medium, secondary UI.
    static var label: UIFont { sans(13, weight: .medium, style: .footnote) }
    /// `--type-label` at regular weight (author lines, metadata rows).
    static var labelRegular: UIFont { sans(13, weight: .regular, style: .footnote) }
    /// `--type-caption` — 12pt sans regular, metadata.
    static var caption: UIFont { sans(12, weight: .regular, style: .caption1) }

    // MARK: Tracking & rhythm

    /// `--tracking-caps` — kern for uppercase eyebrow labels (0.08em).
    static func capsKern(for font: UIFont) -> CGFloat { font.pointSize * 0.08 }

    /// `--type-reading` line height multiple (1.65).
    static let readingLineHeightMultiple: CGFloat = 1.65

    /// `--reading-measure` — max line length for book text (34rem ≈ 544pt).
    static let readingMeasure: CGFloat = 544

    /// Attributed string for an uppercase eyebrow label
    /// (`--type-caption` + `--tracking-caps`).
    static func eyebrow(_ text: String, color: UIColor = InkColor.textSecondary) -> NSAttributedString {
        let font = caption
        return NSAttributedString(
            string: text.uppercased(),
            attributes: [
                .font: font,
                .foregroundColor: color,
                .kern: capsKern(for: font),
            ]
        )
    }

    // MARK: Faces

    /// New York at a Dynamic Type-scaled size.
    static func serif(_ size: CGFloat, weight: UIFont.Weight, style: UIFont.TextStyle) -> UIFont {
        let base = UIFont.systemFont(ofSize: size, weight: weight)
        let face = base.fontDescriptor.withDesign(.serif).map { UIFont(descriptor: $0, size: size) } ?? base
        return UIFontMetrics(forTextStyle: style).scaledFont(for: face)
    }

    /// SF at a Dynamic Type-scaled size.
    static func sans(_ size: CGFloat, weight: UIFont.Weight, style: UIFont.TextStyle) -> UIFont {
        UIFontMetrics(forTextStyle: style).scaledFont(for: .systemFont(ofSize: size, weight: weight))
    }
}

/// Reader body-text sizes (the S / M / L control in the Theme & type sheet).
enum ReadingTextSize: String, CaseIterable {
    case small = "S"
    case medium = "M"
    case large = "L"

    var pointSize: CGFloat {
        switch self {
        case .small: 15
        case .medium: 17
        case .large: 19
        }
    }
}
