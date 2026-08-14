import Foundation

/// Spacing tokens ported from the Inkuna design system
/// (`tokens/spacing.css`).
enum InkSpacing {
    static let space1: CGFloat = 4
    static let space2: CGFloat = 8
    static let space3: CGFloat = 12
    static let space4: CGFloat = 16
    static let space5: CGFloat = 20
    static let space6: CGFloat = 24
    static let space8: CGFloat = 32
    static let space10: CGFloat = 40
    static let space12: CGFloat = 48
    static let space16: CGFloat = 64

    /// `--page-margin` — screen edge gutter.
    static let pageMargin: CGFloat = 20
    /// `--stack-gap` — default vertical rhythm inside cards.
    static let stackGap: CGFloat = 12
}
