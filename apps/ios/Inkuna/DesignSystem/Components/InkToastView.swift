import UIKit

/// Quiet confirmation toast (design-system `Toast`): a dark ink capsule
/// with an accent symbol, floated near the top of the screen.
final class InkToastView: UIView {
    init(symbol: String, text: String) {
        super.init(frame: .zero)
        // Ink stays ink in both themes — the toast is always dark.
        backgroundColor = UIColor(ink: 0x241F17, alpha: 0.92)
        installInkShadow(.lg)

        let glyph = UIImageView(
            image: UIImage(
                systemName: symbol,
                withConfiguration: UIImage.SymbolConfiguration(pointSize: 14, weight: .medium)
            )
        )
        // Fixed night amber: the capsule is always dark, so the dynamic
        // accent would drop contrast in day mode.
        glyph.tintColor = UIColor(ink: 0xD9AE63)

        let label = InkLabel()
        label.text = text
        label.font = InkFont.label
        label.textColor = UIColor(ink: 0xF2EBDD)

        let stack = UIStackView(arrangedSubviews: [glyph, label])
        stack.axis = .horizontal
        stack.spacing = 10
        stack.alignment = .center
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 18),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -18),
            stack.topAnchor.constraint(equalTo: topAnchor, constant: 10),
            stack.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -10),
        ])
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    override func layoutSubviews() {
        super.layoutSubviews()
        layer.cornerRadius = InkRadius.pill(for: bounds.height)
    }

    /// Floats the toast in `view`, fades it in on the quiet curve, and
    /// removes it after `duration`.
    static func show(
        symbol: String,
        text: String,
        in view: UIView,
        topInset: CGFloat,
        duration: TimeInterval = 1.8
    ) {
        // Replace, never stack: repeated taps refresh the toast in place.
        for case let stale as InkToastView in view.subviews {
            stale.removeFromSuperview()
        }
        let toast = InkToastView(symbol: symbol, text: text)
        toast.alpha = 0
        toast.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(toast)
        NSLayoutConstraint.activate([
            toast.centerXAnchor.constraint(equalTo: view.centerXAnchor),
            toast.topAnchor.constraint(equalTo: view.topAnchor, constant: topInset),
        ])
        InkMotion.runQuiet(duration: InkMotion.fast) {
            toast.alpha = 1
        }
        Task { @MainActor in
            try? await Task.sleep(for: .seconds(duration))
            InkMotion.runQuiet(duration: InkMotion.medium) {
                toast.alpha = 0
            } completion: {
                toast.removeFromSuperview()
            }
        }
    }
}
