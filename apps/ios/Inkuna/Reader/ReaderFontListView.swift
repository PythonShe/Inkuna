import UIKit

/// The inline-expanding font list under the Customize panel's Font row:
/// each option leads with an "Aa" specimen set in its own face, and the
/// trailing checkmark's space is always reserved so selection never
/// shifts the layout.
final class ReaderFontListView: UIView {
    var onSelect: ((ReadingFont) -> Void)?

    private var rows: [FontOptionRow] = []
    private let feedback = UISelectionFeedbackGenerator()

    init(selected: ReadingFont) {
        super.init(frame: .zero)

        backgroundColor = InkColor.bgSurface
        layer.cornerRadius = InkRadius.md
        clipsToBounds = true
        installInkShadow(.sm)

        let stack = UIStackView()
        stack.axis = .vertical
        rows = ReadingFont.allCases.map { font in
            let row = FontOptionRow(font: font, chosen: font == selected)
            row.onTap = { [weak self] in self?.pick(font) }
            return row
        }
        for (index, row) in rows.enumerated() {
            if index > 0 { stack.addArrangedSubview(hairline()) }
            stack.addArrangedSubview(row)
        }
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

    private func pick(_ font: ReadingFont) {
        feedback.selectionChanged()
        for row in rows { row.isChosen = row.font == font }
        onSelect?(font)
    }

    private func hairline() -> UIView {
        let separator = UIView()
        separator.backgroundColor = InkColor.borderHairline
        let inset = UIStackView(arrangedSubviews: [separator])
        inset.isLayoutMarginsRelativeArrangement = true
        inset.layoutMargins = UIEdgeInsets(top: 0, left: InkSpacing.space4, bottom: 0, right: 0)
        separator.heightAnchor.constraint(
            equalToConstant: 1 / max(traitCollection.displayScale, 1)
        ).isActive = true
        return inset
    }
}

/// One option: specimen + name + reserved checkmark, accent-soft wash
/// while pressed.
private final class FontOptionRow: UIControl {
    let font: ReadingFont
    var onTap: (() -> Void)?

    var isChosen: Bool {
        didSet { restyle() }
    }

    private let check = UIImageView(
        image: UIImage(
            systemName: "checkmark",
            withConfiguration: UIImage.SymbolConfiguration(pointSize: 15, weight: .semibold)
        )
    )

    init(font: ReadingFont, chosen: Bool) {
        self.font = font
        isChosen = chosen
        super.init(frame: .zero)

        let specimen = InkLabel()
        specimen.text = "Aa"
        specimen.font = font.previewFont(size: 19, bold: false)
        specimen.textColor = InkColor.textDisplay
        specimen.widthAnchor.constraint(equalToConstant: 44).isActive = true

        let name = InkLabel()
        name.text = font.displayName
        name.font = InkFont.ui
        name.textColor = InkColor.textDisplay

        check.tintColor = InkColor.accent
        check.setContentHuggingPriority(.required, for: .horizontal)

        let row = UIStackView(arrangedSubviews: [specimen, name, UIView(), check])
        row.axis = .horizontal
        row.alignment = .center
        row.spacing = InkSpacing.space3
        row.isUserInteractionEnabled = false
        row.translatesAutoresizingMaskIntoConstraints = false
        addSubview(row)
        NSLayoutConstraint.activate([
            row.leadingAnchor.constraint(equalTo: leadingAnchor, constant: InkSpacing.space4),
            row.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -InkSpacing.space4),
            row.topAnchor.constraint(equalTo: topAnchor, constant: 12),
            row.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -12),
        ])

        isAccessibilityElement = true
        let format = NSLocalizedString("a11y_font_option", comment: "")
        accessibilityLabel = String.localizedStringWithFormat(format, font.displayName)
        addAction(UIAction { [weak self] _ in self?.onTap?() }, for: .touchUpInside)
        restyle()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    private func restyle() {
        // The checkmark's space is reserved either way: alpha, not hidden.
        check.alpha = isChosen ? 1 : 0
        accessibilityTraits = isChosen ? [.button, .selected] : .button
    }

    override var isHighlighted: Bool {
        didSet {
            guard isHighlighted != oldValue else { return }
            let pressed = isHighlighted
            InkMotion.runQuiet(duration: InkMotion.fast) {
                self.backgroundColor = pressed ? InkColor.accentSoft : .clear
            }
        }
    }
}
