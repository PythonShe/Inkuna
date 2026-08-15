import UIKit

/// Pill button (design-system `Button`): primary amber fill, soft
/// secondary, and ghost variants in three sizes, with the quiet
/// press-scale the whole system uses.
final class InkButton: UIButton {
    enum Variant {
        case primary
        case secondary
        case ghost
    }

    enum Size {
        case small
        case medium
        case large
    }

    init(
        _ title: String,
        variant: Variant = .primary,
        size: Size = .medium,
        symbol: String? = nil,
        handler: (@MainActor () -> Void)? = nil
    ) {
        super.init(frame: .zero)

        var config = UIButton.Configuration.filled()
        config.cornerStyle = .capsule
        switch variant {
        case .primary:
            config.baseBackgroundColor = InkColor.accentFill
            config.baseForegroundColor = InkColor.accentInk
        case .secondary:
            config.baseBackgroundColor = InkColor.accentSoft
            config.baseForegroundColor = InkColor.accentText
        case .ghost:
            config.baseBackgroundColor = .clear
            config.baseForegroundColor = InkColor.textBody
        }

        let font: UIFont
        switch size {
        case .small:
            config.contentInsets = NSDirectionalEdgeInsets(top: 7, leading: 14, bottom: 7, trailing: 14)
            font = InkFont.label
        case .medium:
            config.contentInsets = NSDirectionalEdgeInsets(top: 10, leading: 20, bottom: 10, trailing: 20)
            font = InkFont.ui
        case .large:
            config.contentInsets = NSDirectionalEdgeInsets(top: 14, leading: 26, bottom: 14, trailing: 26)
            font = InkFont.ui
        }
        config.attributedTitle = AttributedString(title, attributes: AttributeContainer([.font: font]))

        if let symbol {
            config.image = UIImage(
                systemName: symbol,
                withConfiguration: UIImage.SymbolConfiguration(pointSize: font.pointSize, weight: .medium)
            )
            config.imagePadding = InkSpacing.space2
        }

        configuration = config

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
            animator.addAnimations {
                self.transform = pressed ? CGAffineTransform(scaleX: 0.97, y: 0.97) : .identity
            }
            animator.startAnimation()
            pressAnimator = animator
        }
    }
}
