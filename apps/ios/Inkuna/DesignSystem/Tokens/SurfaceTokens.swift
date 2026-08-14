import UIKit

/// Surface tokens ported from the Inkuna design system
/// (`tokens/surface.css`): corner radii and elevation shadows.
enum InkRadius {
    /// Book covers — books stay rectangular.
    static let xs: CGFloat = 4
    static let sm: CGFloat = 8
    static let md: CGFloat = 12
    static let lg: CGFloat = 16
    static let xl: CGFloat = 22
    /// `--radius-pill` — capsule; resolve against the view's height.
    static func pill(for height: CGFloat) -> CGFloat { height / 2 }
}

/// Elevation shadows (`--shadow-*`). CSS double-shadow stacks are
/// approximated by their dominant layer, since `CALayer` carries one shadow.
enum InkShadow {
    case sm
    case md
    case lg
    /// `--shadow-book` — the lifted-off-the-shelf look for covers.
    case book

    func apply(to layer: CALayer, style: UIUserInterfaceStyle) {
        let night = style == .dark
        layer.shadowColor = (night ? UIColor.black : UIColor(ink: 0x1E1911)).cgColor
        switch self {
        case .sm:
            layer.shadowOffset = CGSize(width: 0, height: 1)
            layer.shadowRadius = 1.5
            layer.shadowOpacity = night ? 0.30 : 0.06
        case .md:
            layer.shadowOffset = CGSize(width: 0, height: 4)
            layer.shadowRadius = 8
            layer.shadowOpacity = night ? 0.40 : 0.08
        case .lg:
            layer.shadowOffset = CGSize(width: 0, height: 12)
            layer.shadowRadius = 20
            layer.shadowOpacity = night ? 0.55 : 0.14
        case .book:
            layer.shadowOffset = CGSize(width: 0, height: 10)
            layer.shadowRadius = 12
            layer.shadowOpacity = night ? 0.45 : 0.16
        }
    }
}

extension UIView {
    /// Applies an elevation shadow and keeps it correct across day/night
    /// theme changes. Call once from the view's setup.
    func installInkShadow(_ shadow: InkShadow) {
        shadow.apply(to: layer, style: traitCollection.userInterfaceStyle)
        registerForTraitChanges([UITraitUserInterfaceStyle.self]) { (view: UIView, _) in
            shadow.apply(to: view.layer, style: view.traitCollection.userInterfaceStyle)
        }
    }
}
