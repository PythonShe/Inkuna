import UIKit

/// Floating glass chrome surface — the design's
/// `color-mix(bg-raised 90%, transparent)` + backdrop blur. On iOS 26 it is
/// real Liquid Glass; earlier systems get a blurred material tinted with the
/// raised surface color.
///
/// The view is non-interactive: embed it as the background layer of a
/// control so touches fall through to the control itself (which carries the
/// press feedback).
final class InkGlassView: UIView {
    private let effectView: UIVisualEffectView
    private let fixedCornerRadius: CGFloat?

    /// - Parameter cornerRadius: fixed corner radius, or `nil` for a capsule.
    init(cornerRadius: CGFloat? = nil) {
        fixedCornerRadius = cornerRadius
        if #available(iOS 26.0, *) {
            effectView = UIVisualEffectView(effect: UIGlassEffect())
        } else {
            effectView = UIVisualEffectView(effect: UIBlurEffect(style: .systemThinMaterial))
        }
        super.init(frame: .zero)

        isUserInteractionEnabled = false
        if #available(iOS 26.0, *) {
            // Glass ignores layer.cornerRadius; shape it via the corner
            // configuration instead.
            if let cornerRadius {
                effectView.cornerConfiguration = .uniformCorners(radius: .fixed(cornerRadius))
            } else {
                effectView.cornerConfiguration = .capsule()
            }
        } else {
            effectView.clipsToBounds = true
        }
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
        if #unavailable(iOS 26.0) {
            effectView.layer.cornerRadius = fixedCornerRadius ?? bounds.height / 2
        }
        // An explicit path keeps the shadow off the offscreen-render path:
        // without one, Core Animation re-derives it from the alpha channel
        // of a blurred backdrop — per frame, per surface, exactly while a
        // page is moving underneath the chrome.
        layer.shadowPath = bounds.isEmpty ? nil : UIBezierPath(
            roundedRect: bounds,
            cornerRadius: fixedCornerRadius ?? bounds.height / 2
        ).cgPath
    }
}
