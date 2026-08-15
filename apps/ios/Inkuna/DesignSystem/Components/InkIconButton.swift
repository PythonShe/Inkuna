import UIKit

/// Round 40pt icon button (design-system `IconButton`). The active state
/// gets the accent-soft wash, accent ink, and the filled symbol variant.
final class InkIconButton: UIButton {
    private let symbol: String

    var isActive: Bool = false {
        didSet { setNeedsUpdateConfiguration() }
    }

    init(symbol: String, accessibilityLabel: String, handler: (@MainActor () -> Void)? = nil) {
        self.symbol = symbol
        super.init(frame: .zero)

        var config = UIButton.Configuration.plain()
        config.cornerStyle = .capsule
        config.contentInsets = .zero
        configuration = config
        self.accessibilityLabel = accessibilityLabel

        NSLayoutConstraint.activate([
            widthAnchor.constraint(equalToConstant: 40),
            heightAnchor.constraint(equalToConstant: 40),
        ])

        configurationUpdateHandler = { [weak self] button in
            guard let self else { return }
            var config = button.configuration
            let name = self.isActive ? "\(self.symbol).fill" : self.symbol
            let image = UIImage(
                systemName: name,
                withConfiguration: UIImage.SymbolConfiguration(pointSize: 18, weight: .regular)
            ) ?? UIImage(
                systemName: self.symbol,
                withConfiguration: UIImage.SymbolConfiguration(pointSize: 18, weight: .regular)
            )
            config?.image = image
            config?.baseForegroundColor = self.isActive ? InkColor.accentText : InkColor.textBody
            config?.background.backgroundColor = self.isActive ? InkColor.accentSoft : .clear
            button.configuration = config
        }

        if let handler {
            addAction(UIAction { _ in handler() }, for: .primaryActionTriggered)
        }
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    private var pressAnimator: UIViewPropertyAnimator?

    override var isHighlighted: Bool {
        didSet {
            guard isHighlighted != oldValue else { return }
            let pressed = isHighlighted
            pressAnimator?.stopAnimation(true)
            let animator = InkMotion.quietAnimator(duration: InkMotion.fast)
            animator.addAnimations { self.alpha = pressed ? 0.6 : 1 }
            animator.startAnimation()
            pressAnimator = animator
        }
    }

    // 40pt visual per the design; accept hits out to the 44pt HIG minimum.
    override func point(inside point: CGPoint, with event: UIEvent?) -> Bool {
        bounds.insetBy(dx: -2, dy: -2).contains(point)
    }
}
