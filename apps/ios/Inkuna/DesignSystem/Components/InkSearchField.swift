import UIKit

/// Recessed capsule search field (design-system `SearchField`).
final class InkSearchField: UIView {
    var onTextChange: ((String) -> Void)?

    let textField = UITextField()

    var text: String { textField.text ?? "" }

    init(placeholder: String) {
        super.init(frame: .zero)
        backgroundColor = InkColor.bgRecessed

        let glyph = UIImageView(
            image: UIImage(
                systemName: "magnifyingglass",
                withConfiguration: UIImage.SymbolConfiguration(pointSize: 15, weight: .regular)
            )
        )
        glyph.tintColor = InkColor.textTertiary
        glyph.setContentHuggingPriority(.required, for: .horizontal)

        textField.font = InkFont.sans(15, weight: .regular, style: .callout)
        textField.textColor = InkColor.textBody
        textField.tintColor = InkColor.accent
        textField.attributedPlaceholder = NSAttributedString(
            string: placeholder,
            attributes: [.foregroundColor: InkColor.textTertiary]
        )
        textField.autocorrectionType = .no
        textField.returnKeyType = .search
        textField.addAction(
            UIAction { [weak self] _ in
                guard let self else { return }
                self.onTextChange?(self.text)
            },
            for: .editingChanged
        )

        let stack = UIStackView(arrangedSubviews: [glyph, textField])
        stack.axis = .horizontal
        stack.spacing = 10
        stack.alignment = .center
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: leadingAnchor, constant: InkSpacing.space4),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -InkSpacing.space4),
            stack.topAnchor.constraint(equalTo: topAnchor, constant: 10),
            stack.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -10),
            heightAnchor.constraint(greaterThanOrEqualToConstant: 44),
        ])
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    override func layoutSubviews() {
        super.layoutSubviews()
        layer.cornerRadius = InkRadius.pill(for: bounds.height)
    }
}
