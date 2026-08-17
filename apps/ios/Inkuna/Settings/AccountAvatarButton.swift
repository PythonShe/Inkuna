import UIKit

/// The header's account affordance: a 40pt avatar disc — surface-colored
/// circle, soft shadow, person glyph — top-aligned with the eyebrow/title
/// block, per the design. Sized identically on Android.
final class AccountAvatarButton: UIControl {
    private var pressAnimator: UIViewPropertyAnimator?

    init(handler: @escaping @MainActor () -> Void) {
        super.init(frame: .zero)

        backgroundColor = InkColor.bgSurface
        layer.cornerRadius = 20
        installInkShadow(.sm)

        let glyph = UIImageView(
            image: UIImage(
                systemName: "person.fill",
                withConfiguration: UIImage.SymbolConfiguration(pointSize: 17, weight: .regular)
            )
        )
        glyph.tintColor = InkColor.textSecondary
        glyph.isUserInteractionEnabled = false
        glyph.translatesAutoresizingMaskIntoConstraints = false
        addSubview(glyph)

        NSLayoutConstraint.activate([
            widthAnchor.constraint(equalToConstant: 40),
            heightAnchor.constraint(equalToConstant: 40),
            glyph.centerXAnchor.constraint(equalTo: centerXAnchor),
            glyph.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])

        isAccessibilityElement = true
        accessibilityLabel = String(localized: "settings_title", defaultValue: "Account")
        accessibilityTraits = .button

        // .touchUpInside, not .primaryActionTriggered: plain UIControl
        // subclasses never emit the latter.
        addAction(UIAction { _ in handler() }, for: .touchUpInside)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    override var isHighlighted: Bool {
        didSet {
            guard isHighlighted != oldValue else { return }
            let pressed = isHighlighted
            pressAnimator?.stopAnimation(true)
            let animator = InkMotion.quietAnimator(duration: InkMotion.fast)
            animator.addAnimations {
                self.transform = pressed ? CGAffineTransform(scaleX: 0.94, y: 0.94) : .identity
            }
            animator.startAnimation()
            pressAnimator = animator
        }
    }

    // 40pt visual per the design; accept hits out to the 44pt HIG minimum.
    override func point(inside point: CGPoint, with event: UIEvent?) -> Bool {
        bounds.insetBy(dx: -2, dy: -2).contains(point)
    }
}
