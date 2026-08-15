import UIKit

/// Filter chip (design-system `Chip`). Selected chips fill with accent;
/// unselected ones sit on the surface behind a hairline.
final class InkChip: UIButton {
    var isChosen: Bool {
        didSet { setNeedsUpdateConfiguration() }
    }

    init(_ title: String, chosen: Bool = false, handler: (@MainActor () -> Void)? = nil) {
        isChosen = chosen
        super.init(frame: .zero)

        var config = UIButton.Configuration.filled()
        config.cornerStyle = .capsule
        config.contentInsets = NSDirectionalEdgeInsets(top: 7, leading: 14, bottom: 7, trailing: 14)
        config.attributedTitle = AttributedString(title, attributes: AttributeContainer([.font: InkFont.label]))
        config.background.strokeWidth = 1
        configuration = config

        configurationUpdateHandler = { [weak self] button in
            guard let self else { return }
            var config = button.configuration
            config?.baseBackgroundColor = self.isChosen ? InkColor.accentFill : InkColor.bgSurface
            config?.baseForegroundColor = self.isChosen ? InkColor.accentInk : InkColor.textSecondary
            config?.background.strokeColor = self.isChosen ? .clear : InkColor.borderHairline
            button.configuration = config
        }

        if let handler {
            addAction(UIAction { _ in handler() }, for: .primaryActionTriggered)
        }
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }
}
