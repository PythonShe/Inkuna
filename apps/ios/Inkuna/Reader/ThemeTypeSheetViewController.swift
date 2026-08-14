import UIKit

/// "Theme & type" bottom sheet: page brightness, the four reading themes,
/// and the S/M/L text size — presented as a native detent sheet over the
/// reader.
final class ThemeTypeSheetViewController: UIViewController {
    var onThemeChange: ((ReadingTheme) -> Void)?
    var onSizeChange: ((ReadingTextSize) -> Void)?
    var onBrightnessChange: ((Double) -> Void)?

    private var swatches: [ThemeSwatchButton] = []
    private let selectionFeedback = UISelectionFeedbackGenerator()

    init() {
        super.init(nibName: nil, bundle: nil)
        modalPresentationStyle = .pageSheet
        if let sheet = sheetPresentationController {
            sheet.detents = [.custom { _ in 292 }]
            sheet.prefersGrabberVisible = true
            sheet.preferredCornerRadius = InkRadius.lg
        }
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = InkColor.bgSurface
        let settings = AppSettings.shared

        // TODO(l10n): localize once the strings pass lands.
        let titleLabel = UILabel()
        titleLabel.text = "Theme & type"
        titleLabel.font = InkFont.heading
        titleLabel.textColor = InkColor.textDisplay

        var closeConfig = UIButton.Configuration.filled()
        closeConfig.cornerStyle = .capsule
        closeConfig.baseBackgroundColor = InkColor.bgRecessed
        closeConfig.baseForegroundColor = InkColor.textSecondary
        closeConfig.image = UIImage(
            systemName: "xmark",
            withConfiguration: UIImage.SymbolConfiguration(pointSize: 11, weight: .semibold)
        )
        let closeButton = UIButton(configuration: closeConfig)
        closeButton.addAction(UIAction { [weak self] _ in self?.dismiss(animated: true) }, for: .primaryActionTriggered)
        NSLayoutConstraint.activate([
            closeButton.widthAnchor.constraint(equalToConstant: 28),
            closeButton.heightAnchor.constraint(equalToConstant: 28),
        ])

        let titleRow = UIStackView(arrangedSubviews: [titleLabel, UIView(), closeButton])
        titleRow.axis = .horizontal
        titleRow.alignment = .center

        // Brightness: an ink veil over the page, not system brightness.
        let dimGlyph = sliderGlyph("sun.min")
        let brightGlyph = sliderGlyph("sun.max")
        let slider = UISlider()
        slider.minimumTrackTintColor = InkColor.accent
        slider.maximumTrackTintColor = InkColor.bgRecessed
        slider.value = Float(settings.brightness)
        slider.addAction(
            UIAction { [weak self] action in
                guard let slider = action.sender as? UISlider else { return }
                self?.onBrightnessChange?(Double(slider.value))
            },
            for: .valueChanged
        )
        let sliderRow = UIStackView(arrangedSubviews: [dimGlyph, slider, brightGlyph])
        sliderRow.axis = .horizontal
        sliderRow.spacing = InkSpacing.space3
        sliderRow.alignment = .center

        swatches = ReadingTheme.allCases.map { theme in
            let swatch = ThemeSwatchButton(theme: theme, chosen: theme == settings.readingTheme)
            swatch.onPick = { [weak self] picked in
                guard let self else { return }
                self.selectionFeedback.selectionChanged()
                for swatch in self.swatches {
                    swatch.isChosen = swatch.theme == picked
                }
                self.onThemeChange?(picked)
            }
            return swatch
        }
        let swatchRow = UIStackView(arrangedSubviews: swatches)
        swatchRow.axis = .horizontal
        swatchRow.spacing = 10
        swatchRow.distribution = .fillEqually

        let sizeControl = InkSegmentedControl(
            options: ReadingTextSize.allCases.map(\.rawValue),
            selected: settings.textSize.rawValue
        )
        sizeControl.onChange = { [weak self] option in
            guard let size = ReadingTextSize(rawValue: option) else { return }
            self?.onSizeChange?(size)
        }
        let sizeRow = UIStackView(arrangedSubviews: [sizeControl])
        sizeRow.axis = .vertical
        sizeRow.alignment = .center
        NSLayoutConstraint.activate([
            sizeControl.widthAnchor.constraint(equalToConstant: 180)
        ])

        let stack = UIStackView(arrangedSubviews: [titleRow, sliderRow, swatchRow, sizeRow])
        stack.axis = .vertical
        stack.spacing = 18
        stack.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(stack)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: InkSpacing.pageMargin),
            stack.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -InkSpacing.pageMargin),
            stack.topAnchor.constraint(equalTo: view.topAnchor, constant: InkSpacing.space6),
        ])
    }

    private func sliderGlyph(_ symbol: String) -> UIImageView {
        let view = UIImageView(
            image: UIImage(
                systemName: symbol,
                withConfiguration: UIImage.SymbolConfiguration(pointSize: 15, weight: .regular)
            )
        )
        view.tintColor = InkColor.textTertiary
        view.setContentHuggingPriority(.required, for: .horizontal)
        return view
    }
}
