import UIKit

/// 46pt floating circular glass button — the reader's back button, menu
/// button, and the round entries of the speed-dial menu.
final class ReaderGlassButton: UIControl {
    private let glass = InkGlassView()
    private let iconView = UIImageView()
    private let symbolPointSize: CGFloat
    private var pressAnimator: UIViewPropertyAnimator?

    init(
        symbol: String,
        pointSize: CGFloat = 17,
        accessibilityLabel: String,
        handler: @escaping @MainActor () -> Void
    ) {
        symbolPointSize = pointSize
        super.init(frame: .zero)

        glass.translatesAutoresizingMaskIntoConstraints = false
        addSubview(glass)

        iconView.image = UIImage(
            systemName: symbol,
            withConfiguration: UIImage.SymbolConfiguration(pointSize: pointSize, weight: .regular)
        )
        iconView.tintColor = InkColor.textDisplay
        iconView.translatesAutoresizingMaskIntoConstraints = false
        addSubview(iconView)

        NSLayoutConstraint.activate([
            widthAnchor.constraint(equalToConstant: 46),
            heightAnchor.constraint(equalToConstant: 46),
            glass.leadingAnchor.constraint(equalTo: leadingAnchor),
            glass.trailingAnchor.constraint(equalTo: trailingAnchor),
            glass.topAnchor.constraint(equalTo: topAnchor),
            glass.bottomAnchor.constraint(equalTo: bottomAnchor),
            iconView.centerXAnchor.constraint(equalTo: centerXAnchor),
            iconView.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])

        isAccessibilityElement = true
        self.accessibilityLabel = accessibilityLabel
        accessibilityTraits = .button

        // .touchUpInside, not .primaryActionTriggered: plain UIControl
        // subclasses never emit the latter.
        addAction(UIAction { _ in handler() }, for: .touchUpInside)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    /// Swaps the glyph with the native symbol replace transition.
    func setSymbol(_ symbol: String) {
        guard let image = UIImage(
            systemName: symbol,
            withConfiguration: UIImage.SymbolConfiguration(pointSize: symbolPointSize, weight: .regular)
        ) else { return }
        iconView.setSymbolImage(image, contentTransition: .replace.downUp)
    }

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

/// Pill-shaped row of the reader's speed-dial menu: label on the left,
/// glyph on the right, glass beneath.
final class ReaderMenuPill: UIControl {
    private let glass = InkGlassView()
    private let titleLabel = InkLabel()
    private var pressAnimator: UIViewPropertyAnimator?

    var text: String? {
        get { titleLabel.text }
        set {
            titleLabel.text = newValue
            accessibilityLabel = newValue
        }
    }

    init(text: String, symbol: String, handler: @escaping @MainActor () -> Void) {
        super.init(frame: .zero)

        glass.translatesAutoresizingMaskIntoConstraints = false
        addSubview(glass)

        titleLabel.text = text
        titleLabel.font = InkFont.ui
        titleLabel.textColor = InkColor.textDisplay
        // Wrap at accessibility sizes instead of running off the screen.
        titleLabel.numberOfLines = 0

        let iconView = UIImageView(
            image: UIImage(
                systemName: symbol,
                withConfiguration: UIImage.SymbolConfiguration(pointSize: 17, weight: .regular)
            )
        )
        iconView.tintColor = InkColor.textSecondary
        iconView.setContentHuggingPriority(.required, for: .horizontal)

        let row = UIStackView(arrangedSubviews: [titleLabel, iconView])
        row.axis = .horizontal
        row.alignment = .center
        row.spacing = 14
        row.isUserInteractionEnabled = false
        row.translatesAutoresizingMaskIntoConstraints = false
        addSubview(row)

        NSLayoutConstraint.activate([
            glass.leadingAnchor.constraint(equalTo: leadingAnchor),
            glass.trailingAnchor.constraint(equalTo: trailingAnchor),
            glass.topAnchor.constraint(equalTo: topAnchor),
            glass.bottomAnchor.constraint(equalTo: bottomAnchor),
            widthAnchor.constraint(greaterThanOrEqualToConstant: 198),
            row.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 18),
            row.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -15),
            row.topAnchor.constraint(equalTo: topAnchor, constant: 13),
            row.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -13),
        ])

        isAccessibilityElement = true
        accessibilityLabel = text
        accessibilityTraits = .button

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
                self.transform = pressed ? CGAffineTransform(scaleX: 0.98, y: 0.98) : .identity
                self.alpha = pressed ? 0.85 : 1
            }
            animator.startAnimation()
            pressAnimator = animator
        }
    }
}

/// The fanned-out reading menu above the more button: Contents · %,
/// Theme & type, then the bookmark circle. No search affordance until the
/// core's real, CJK-aware search lands — offering the placeholder panel's
/// invented results against a real book would be worse than offering
/// nothing.
final class ReaderMenuView: UIView {
    let contentsPill: ReaderMenuPill

    init(
        onContents: @escaping @MainActor () -> Void,
        onTheme: @escaping @MainActor () -> Void,
        onBookmark: @escaping @MainActor () -> Void
    ) {
        // TODO(l10n): localize once the strings pass lands.
        contentsPill = ReaderMenuPill(text: "Contents", symbol: "list.bullet", handler: onContents)
        super.init(frame: .zero)

        let themePill = ReaderMenuPill(text: "Theme & type", symbol: "textformat.size", handler: onTheme)
        let bookmarkButton = ReaderGlassButton(symbol: "bookmark", accessibilityLabel: "Place bookmark", handler: onBookmark)

        let circleRow = UIStackView(arrangedSubviews: [bookmarkButton])
        circleRow.axis = .horizontal
        circleRow.spacing = 10

        let stack = UIStackView(arrangedSubviews: [contentsPill, themePill, circleRow])
        stack.axis = .vertical
        stack.alignment = .trailing
        stack.spacing = 10
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor),
            stack.topAnchor.constraint(equalTo: topAnchor),
            stack.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }
}
