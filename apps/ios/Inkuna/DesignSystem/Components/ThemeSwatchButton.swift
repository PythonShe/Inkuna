import UIKit

/// Reading-theme swatch (design-system `ThemeSwatch`): a card painted in
/// the theme's own page colors with an "Aa" specimen. Selection draws the
/// accent ring and lifts the card.
final class ThemeSwatchButton: UIControl {
    let theme: ReadingTheme

    var isChosen: Bool {
        didSet { restyle() }
    }

    var onPick: ((ReadingTheme) -> Void)?

    private let specimen = UILabel()
    private let nameLabel = UILabel()

    init(theme: ReadingTheme, chosen: Bool = false) {
        self.theme = theme
        self.isChosen = chosen
        super.init(frame: .zero)

        backgroundColor = theme.background
        layer.cornerRadius = InkRadius.md
        layer.borderWidth = 2
        registerForTraitChanges([UITraitUserInterfaceStyle.self]) { (view: ThemeSwatchButton, _) in
            view.restyle()
        }

        specimen.text = "Aa"
        specimen.font = InkFont.serif(22, weight: .regular, style: .title3)
        specimen.textColor = theme.foreground

        nameLabel.text = theme.displayName
        nameLabel.font = InkFont.sans(11, weight: .regular, style: .caption2)
        nameLabel.textColor = theme.foreground.withAlphaComponent(0.75)

        for label in [specimen, nameLabel] {
            label.isUserInteractionEnabled = false
            label.translatesAutoresizingMaskIntoConstraints = false
            addSubview(label)
        }
        NSLayoutConstraint.activate([
            specimen.leadingAnchor.constraint(equalTo: leadingAnchor, constant: InkSpacing.space3),
            specimen.topAnchor.constraint(equalTo: topAnchor, constant: 14),
            nameLabel.leadingAnchor.constraint(equalTo: leadingAnchor, constant: InkSpacing.space3),
            nameLabel.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -10),
            nameLabel.topAnchor.constraint(greaterThanOrEqualTo: specimen.bottomAnchor, constant: InkSpacing.space4),
            heightAnchor.constraint(greaterThanOrEqualToConstant: 70),
        ])

        accessibilityLabel = "\(theme.displayName). \(theme.subtitle)"
        accessibilityTraits = .button

        addAction(UIAction { [weak self] _ in
            guard let self else { return }
            self.onPick?(self.theme)
        }, for: .primaryActionTriggered)

        restyle()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    private func restyle() {
        layer.borderColor = isChosen ? InkColor.accent.resolvedColor(with: traitCollection).cgColor : UIColor.clear.cgColor
        (isChosen ? InkShadow.md : InkShadow.sm).apply(to: layer, style: traitCollection.userInterfaceStyle)
    }
}
