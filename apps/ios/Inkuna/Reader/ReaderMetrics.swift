import UIKit

/// The reader's vertical reading band, mirrored verbatim by the Android
/// shell (`apps/android/.../ui/reader/ReaderMetrics.kt`): both platforms
/// must render the same air around the page on comparable hardware.
/// Change a value here, change its sibling too.
///
/// Phones pad relative to the system insets — the top inset already spans
/// the status bar or Dynamic Island, so the bonus is pure breathing room —
/// with floors so inset-less devices never feel cramped. Tablets have no
/// cutout and a broad canvas: a fixed, generous band, not an inset-driven
/// one.
enum ReaderMetrics {
    /// Air between the top system inset and the first line on phones.
    static let phoneTopBonus: CGFloat = 12
    /// Floor for the top band on phones with small top insets.
    static let phoneTopMinimum: CGFloat = 40
    /// Air between the bottom system inset and the last line on phones —
    /// wide enough that the page-info footer lives inside the band.
    static let phoneBottomBonus: CGFloat = 28
    /// Floor for the bottom band on phones without a home indicator.
    static let phoneBottomMinimum: CGFloat = 48
    /// Fixed top and bottom band on tablets.
    static let tabletVertical: CGFloat = 64
    /// The footer's bottom edge sits this far above the bottom inset.
    static let footerLift: CGFloat = 6

    static func contentInsets(safeArea: UIEdgeInsets, isPad: Bool) -> UIEdgeInsets {
        guard !isPad else {
            return UIEdgeInsets(top: tabletVertical, left: 0, bottom: tabletVertical, right: 0)
        }
        return UIEdgeInsets(
            top: max(safeArea.top + phoneTopBonus, phoneTopMinimum),
            left: 0,
            bottom: max(safeArea.bottom + phoneBottomBonus, phoneBottomMinimum),
            right: 0
        )
    }
}
