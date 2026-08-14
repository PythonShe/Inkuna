import UIKit

/// Floating capsule tab bar (design-system `TabBar`).
///
/// On iOS 26+ the pill is Liquid Glass; earlier systems get the chrome
/// material blur. The selected item sits on an accent-soft capsule with a
/// filled symbol.
final class InkTabBar: UIView {
    struct Item: Equatable {
        let id: String
        let symbol: String
        let title: String
    }

    var onSelect: ((String) -> Void)?

    private(set) var selectedID: String
    private let items: [Item]
    private let stack = UIStackView()
    private let selectionFeedback = UISelectionFeedbackGenerator()

    init(items: [Item], selectedID: String) {
        self.items = items
        self.selectedID = selectedID
        super.init(frame: .zero)

        let background = UIVisualEffectView()
        if #available(iOS 26.0, *) {
            let glass = UIGlassEffect()
            glass.isInteractive = true
            background.effect = glass
        } else {
            background.effect = UIBlurEffect(style: .systemChromeMaterial)
        }
        background.layer.masksToBounds = true
        background.translatesAutoresizingMaskIntoConstraints = false
        addSubview(background)

        installInkShadow(.md)

        stack.axis = .horizontal
        stack.spacing = 4
        stack.translatesAutoresizingMaskIntoConstraints = false
        background.contentView.addSubview(stack)

        NSLayoutConstraint.activate([
            background.leadingAnchor.constraint(equalTo: leadingAnchor),
            background.trailingAnchor.constraint(equalTo: trailingAnchor),
            background.topAnchor.constraint(equalTo: topAnchor),
            background.bottomAnchor.constraint(equalTo: bottomAnchor),
            stack.leadingAnchor.constraint(equalTo: background.contentView.leadingAnchor, constant: 10),
            stack.trailingAnchor.constraint(equalTo: background.contentView.trailingAnchor, constant: -10),
            stack.topAnchor.constraint(equalTo: background.contentView.topAnchor, constant: 8),
            stack.bottomAnchor.constraint(equalTo: background.contentView.bottomAnchor, constant: -8),
        ])

        self.background = background

        for item in items {
            let button = UIButton(type: .custom)
            var config = UIButton.Configuration.filled()
            config.cornerStyle = .capsule
            config.contentInsets = NSDirectionalEdgeInsets(top: 8, leading: 16, bottom: 8, trailing: 16)
            config.imagePlacement = .top
            config.imagePadding = 2
            config.attributedTitle = AttributedString(
                item.title,
                attributes: AttributeContainer([.font: InkFont.sans(10, weight: .regular, style: .caption2)])
            )
            button.configuration = config
            button.accessibilityLabel = item.title
            button.configurationUpdateHandler = { [weak self] button in
                guard let self else { return }
                let isSelected = item.id == self.selectedID
                var config = button.configuration
                let symbolName = isSelected ? "\(item.symbol).fill" : item.symbol
                config?.image = UIImage(
                    systemName: symbolName,
                    withConfiguration: UIImage.SymbolConfiguration(pointSize: 17, weight: .regular)
                ) ?? UIImage(
                    systemName: item.symbol,
                    withConfiguration: UIImage.SymbolConfiguration(pointSize: 17, weight: .regular)
                )
                config?.baseBackgroundColor = isSelected ? InkColor.accentSoft : .clear
                config?.baseForegroundColor = isSelected ? InkColor.accent : InkColor.textSecondary
                button.configuration = config
            }
            button.addAction(
                UIAction { [weak self] _ in self?.select(item.id) },
                for: .primaryActionTriggered
            )
            stack.addArrangedSubview(button)
        }
    }

    private weak var background: UIVisualEffectView?

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    override func layoutSubviews() {
        super.layoutSubviews()
        background?.layer.cornerRadius = InkRadius.pill(for: bounds.height)
    }

    func select(_ id: String, notify: Bool = true) {
        guard items.contains(where: { $0.id == id }), id != selectedID else { return }
        selectedID = id
        for case let button as UIButton in stack.arrangedSubviews {
            button.setNeedsUpdateConfiguration()
        }
        if notify {
            selectionFeedback.selectionChanged()
            onSelect?(id)
        }
    }
}
