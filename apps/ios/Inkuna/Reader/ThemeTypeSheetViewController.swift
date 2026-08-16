import UIKit

/// "Theme & type" bottom sheet: the A− / A+ text-size stepper, page
/// brightness, and the four reading themes as a 2×2 tile grid — presented
/// as a native detent sheet over the reader.
final class ThemeTypeSheetViewController: UIViewController {
    var onThemeChange: ((ReadingTheme) -> Void)?
    var onSizeChange: ((ReadingTextSize) -> Void)?
    var onBrightnessChange: ((Double) -> Void)?

    private var tiles: [ThemeTileButton] = []
    private var contentStack: UIStackView?
    private var textSize = AppSettings.shared.textSize
    private let sizeDownButton = UIButton(configuration: .plain())
    private let sizeUpButton = UIButton(configuration: .plain())
    private let selectionFeedback = UISelectionFeedbackGenerator()

    init() {
        super.init(nibName: nil, bundle: nil)
        modalPresentationStyle = .pageSheet
        if let sheet = sheetPresentationController {
            // Fit the content: at accessibility text sizes a fixed design
            // height would clip the theme grid.
            sheet.detents = [
                .custom { [weak self] context in
                    guard let self else { return 320 }
                    self.loadViewIfNeeded()
                    guard let stack = self.contentStack else { return 320 }
                    let fitted = stack.systemLayoutSizeFitting(UIView.layoutFittingCompressedSize).height + 56
                    return min(max(fitted, 320), context.maximumDetentValue)
                },
            ]
            sheet.prefersGrabberVisible = true
            sheet.preferredCornerRadius = InkRadius.lg
        }
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = InkColor.bgSurface
        selectionFeedback.prepare()
        let settings = AppSettings.shared

        let titleLabel = InkLabel()
        titleLabel.text = String(localized: "reader_menu_theme_type", defaultValue: "Theme & type")
        titleLabel.font = InkFont.heading
        titleLabel.textColor = InkColor.textDisplay

        let closeButton = InkCloseButton { [weak self] in self?.dismiss(animated: true) }

        let titleRow = UIStackView(arrangedSubviews: [titleLabel, UIView(), closeButton])
        titleRow.axis = .horizontal
        titleRow.alignment = .center

        // Text size: the split A− / A+ stepper pill.
        let stepper = makeSizeStepper()
        updateStepperState()

        // Brightness: an ink veil over the page, not system brightness.
        let dimGlyph = sliderGlyph("sun.min")
        let brightGlyph = sliderGlyph("sun.max")
        let slider = UISlider()
        slider.accessibilityLabel = String(localized: "a11y_page_brightness", defaultValue: "Page brightness")
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

        // Themes: 2×2 grid of page-colored tiles.
        tiles = ReadingTheme.allCases.map { theme in
            let tile = ThemeTileButton(theme: theme, chosen: theme == settings.readingTheme)
            tile.onPick = { [weak self] picked in
                guard let self else { return }
                self.selectionFeedback.selectionChanged()
                for tile in self.tiles {
                    tile.isChosen = tile.theme == picked
                }
                self.onThemeChange?(picked)
            }
            return tile
        }
        let gridTop = UIStackView(arrangedSubviews: [tiles[0], tiles[1]])
        let gridBottom = UIStackView(arrangedSubviews: [tiles[2], tiles[3]])
        for gridRow in [gridTop, gridBottom] {
            gridRow.axis = .horizontal
            gridRow.spacing = 10
            gridRow.distribution = .fillEqually
        }
        let grid = UIStackView(arrangedSubviews: [gridTop, gridBottom])
        grid.axis = .vertical
        grid.spacing = 10

        let stack = UIStackView(arrangedSubviews: [titleRow, stepper, sliderRow, grid])
        stack.axis = .vertical
        stack.spacing = 16
        stack.setCustomSpacing(14, after: stepper)
        stack.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(stack)
        contentStack = stack
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: InkSpacing.pageMargin),
            stack.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -InkSpacing.pageMargin),
            stack.topAnchor.constraint(equalTo: view.topAnchor, constant: InkSpacing.space6),
            stack.bottomAnchor.constraint(lessThanOrEqualTo: view.safeAreaLayoutGuide.bottomAnchor, constant: -InkSpacing.space4),
        ])
    }

    // MARK: Size stepper

    private func makeSizeStepper() -> UIView {
        let container = UIView()
        container.backgroundColor = InkColor.bgRecessed
        container.layer.cornerRadius = 20

        configureStepButton(
            sizeDownButton,
            sample: InkFont.serif(13.5, weight: .medium, style: .footnote),
            accessibilityLabel: String(localized: "a11y_smaller_text", defaultValue: "Smaller text")
        )
        configureStepButton(
            sizeUpButton,
            sample: InkFont.serif(19, weight: .medium, style: .body),
            accessibilityLabel: String(localized: "a11y_larger_text", defaultValue: "Larger text")
        )
        sizeDownButton.addAction(UIAction { [weak self] _ in self?.step(-1) }, for: .primaryActionTriggered)
        sizeUpButton.addAction(UIAction { [weak self] _ in self?.step(1) }, for: .primaryActionTriggered)

        let divider = UIView()
        divider.backgroundColor = InkColor.borderHairline
        divider.translatesAutoresizingMaskIntoConstraints = false

        let row = UIStackView(arrangedSubviews: [sizeDownButton, sizeUpButton])
        row.axis = .horizontal
        row.distribution = .fillEqually
        row.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(row)
        container.addSubview(divider)
        NSLayoutConstraint.activate([
            container.heightAnchor.constraint(equalToConstant: 40),
            row.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            row.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            row.topAnchor.constraint(equalTo: container.topAnchor),
            row.bottomAnchor.constraint(equalTo: container.bottomAnchor),
            divider.centerXAnchor.constraint(equalTo: container.centerXAnchor),
            divider.widthAnchor.constraint(equalToConstant: 1),
            divider.topAnchor.constraint(equalTo: container.topAnchor, constant: 8),
            divider.bottomAnchor.constraint(equalTo: container.bottomAnchor, constant: -8),
        ])
        return container
    }

    private func configureStepButton(_ button: UIButton, sample: UIFont, accessibilityLabel: String) {
        var config = button.configuration
        config?.attributedTitle = AttributedString("A", attributes: AttributeContainer([.font: sample]))
        button.configuration = config
        button.accessibilityLabel = accessibilityLabel
        button.configurationUpdateHandler = { button in
            var config = button.configuration
            config?.baseForegroundColor = button.isEnabled ? InkColor.textDisplay : InkColor.textTertiary
            button.configuration = config
        }
    }

    private func step(_ delta: Int) {
        guard let next = ReadingTextSize(rawValue: textSize.rawValue + delta) else { return }
        textSize = next
        selectionFeedback.selectionChanged()
        updateStepperState()
        onSizeChange?(next)
    }

    private func updateStepperState() {
        sizeDownButton.isEnabled = textSize.smaller != nil
        sizeUpButton.isEnabled = textSize.larger != nil
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

/// One tile of the theme grid: the theme's own page colors with an "Aa"
/// specimen, ringed in accent when chosen.
private final class ThemeTileButton: UIControl {
    let theme: ReadingTheme

    var onPick: ((ReadingTheme) -> Void)?

    var isChosen: Bool {
        didSet { restyle() }
    }

    private var pressAnimator: UIViewPropertyAnimator?

    init(theme: ReadingTheme, chosen: Bool) {
        self.theme = theme
        isChosen = chosen
        super.init(frame: .zero)

        backgroundColor = theme.background
        layer.cornerRadius = InkRadius.md
        layer.borderWidth = 2
        installInkShadow(.sm)

        let specimen = InkLabel()
        specimen.text = "Aa"
        specimen.font = InkFont.serif(21.5, weight: .medium, style: .title3)
        specimen.textColor = theme.foreground

        let name = InkLabel()
        name.text = theme.displayName
        name.font = InkFont.caption
        name.textColor = theme.foreground.withAlphaComponent(0.75)

        let row = UIStackView(arrangedSubviews: [specimen, name])
        row.axis = .horizontal
        row.alignment = .lastBaseline
        row.distribution = .equalSpacing
        row.isUserInteractionEnabled = false
        row.translatesAutoresizingMaskIntoConstraints = false
        addSubview(row)
        NSLayoutConstraint.activate([
            row.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 16),
            row.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -16),
            row.topAnchor.constraint(equalTo: topAnchor, constant: 14),
            row.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -14),
        ])

        isAccessibilityElement = true
        let tileFormat = NSLocalizedString("a11y_theme_tile", comment: "")
        accessibilityLabel = String.localizedStringWithFormat(tileFormat, theme.displayName, theme.subtitle)
        accessibilityTraits = .button

        registerForTraitChanges([UITraitUserInterfaceStyle.self]) { (tile: ThemeTileButton, _) in
            tile.restyle()
        }
        // .touchUpInside, not .primaryActionTriggered: plain UIControl
        // subclasses never emit the latter.
        addAction(UIAction { [weak self] _ in
            guard let self else { return }
            self.onPick?(self.theme)
        }, for: .touchUpInside)

        restyle()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    private func restyle() {
        let accent = InkColor.accent.resolvedColor(with: traitCollection)
        layer.borderColor = isChosen ? accent.cgColor : UIColor.clear.cgColor
        accessibilityTraits = isChosen ? [.button, .selected] : .button
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
