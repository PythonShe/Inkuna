import UIKit

/// 32pt recessed circular close button used by the reader overlays and the
/// Theme & type sheet.
final class InkCloseButton: UIButton {
    init(handler: @escaping @MainActor () -> Void) {
        super.init(frame: .zero)

        var config = UIButton.Configuration.filled()
        config.cornerStyle = .capsule
        config.baseBackgroundColor = InkColor.bgRecessed
        config.baseForegroundColor = InkColor.textSecondary
        config.image = UIImage(
            systemName: "xmark",
            withConfiguration: UIImage.SymbolConfiguration(pointSize: 12, weight: .semibold)
        )
        configuration = config
        // TODO(l10n): localize once the strings pass lands.
        accessibilityLabel = "Close"

        NSLayoutConstraint.activate([
            widthAnchor.constraint(equalToConstant: 32),
            heightAnchor.constraint(equalToConstant: 32),
        ])

        addAction(UIAction { _ in handler() }, for: .primaryActionTriggered)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    // 32pt visual per the design; accept hits out to the 44pt HIG minimum.
    override func point(inside point: CGPoint, with event: UIEvent?) -> Bool {
        bounds.insetBy(dx: -6, dy: -6).contains(point)
    }
}
