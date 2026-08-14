import UIKit

/// UILabel that tracks Dynamic Type at runtime. Every label in the app
/// uses this so the `UIFontMetrics`-scaled token fonts resize live when
/// the user changes text size, not just at next launch.
final class InkLabel: UILabel {
    override init(frame: CGRect) {
        super.init(frame: frame)
        adjustsFontForContentSizeCategory = true
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }
}
