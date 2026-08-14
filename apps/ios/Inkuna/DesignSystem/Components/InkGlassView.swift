import UIKit

/// Floating glass chrome surface — the design's
/// `color-mix(bg-raised 90%, transparent)` + backdrop blur. On iOS 26 it is
/// real Liquid Glass; earlier systems get a blurred material tinted with the
/// raised surface color.
///
/// The view is non-interactive: embed it as the background layer of a
/// control so touches fall through to the control itself.
final class InkGlassView: UIView {
    private let effectView: UIVisualEffectView
    private let fixedCornerRadius: CGFloat?

    /// - Parameters:
    ///   - cornerRadius: fixed corner radius, or `nil` for a capsule.
    ///   - interactive: on iOS 26, lets the glass react to touch.
    init(cornerRadius: CGFloat? = nil, interactive: Bool = false) {
        fixedCornerRadius = cornerRadius
        if #available(iOS 26.0, *) {
            let glass = UIGlassEffect()
            glass.isInteractive = interactive
            effectView = UIVisualEffectView(effect: glass)
        } else {
            effectView = UIVisualEffectView(effect: UIBlurEffect(style: .systemThinMaterial))
        }
        super.init(frame: .zero)

        isUserInteractionEnabled = false
        effectView.clipsToBounds = true
        effectView.translatesAutoresizingMaskIntoConstraints = false
        addSubview(effectView)
        NSLayoutConstraint.activate([
            effectView.leadingAnchor.constraint(equalTo: leadingAnchor),
            effectView.trailingAnchor.constraint(equalTo: trailingAnchor),
            effectView.topAnchor.constraint(equalTo: topAnchor),
            effectView.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])

        if #unavailable(iOS 26.0) {
            let tint = UIView()
            tint.backgroundColor = InkColor.bgRaised.withAlphaComponent(0.55)
            tint.translatesAutoresizingMaskIntoConstraints = false
            effectView.contentView.addSubview(tint)
            NSLayoutConstraint.activate([
                tint.leadingAnchor.constraint(equalTo: effectView.contentView.leadingAnchor),
                tint.trailingAnchor.constraint(equalTo: effectView.contentView.trailingAnchor),
                tint.topAnchor.constraint(equalTo: effectView.contentView.topAnchor),
                tint.bottomAnchor.constraint(equalTo: effectView.contentView.bottomAnchor),
            ])
        }

        installInkShadow(.md)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    override func layoutSubviews() {
        super.layoutSubviews()
        effectView.layer.cornerRadius = fixedCornerRadius ?? bounds.height / 2
    }
}
