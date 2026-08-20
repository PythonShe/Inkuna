import UIKit

/// The Customize panel — level 2 of the Theme & type sheet. A live
/// preview on the reading surface, the font roster, the bold toggle, and
/// the four layout sliders; everything applies through the reader's own
/// stylesheet, previewing per step and committing on release.
final class ReaderCustomizeViewController: UIViewController, ReaderSheetPage {
    /// A slider touch landed: the reader captures its reflow anchor and
    /// opens the live session (progress writes pause).
    var onSessionBegin: ((ReaderUserStyle) -> Void)?
    /// Live, uncommitted style — CSS only, no persistence.
    var onPreview: ((ReaderUserStyle) -> Void)?
    /// Committed — the reader persists and applies with a final re-land.
    var onCommit: ((ReaderUserStyle) -> Void)?
    /// Close (not back): dismiss the whole sheet to the reader.
    var onClose: (() -> Void)?

    var sheetDetents: [UISheetPresentationController.Detent] { [.large()] }
    var preferredDetentIdentifier: UISheetPresentationController.Detent.Identifier? { .large }

    private var style: ReaderUserStyle
    private let theme: ReadingTheme
    private let textSize: ReadingTextSize
    private let fallbackPhrase: String
    private let phraseProvider: () async -> String?

    private var previewCard: ReaderPreviewCard?
    private var fontValueLabel: InkLabel?
    private var fontRow: MenuRowControl?
    private var sliders: [InkStepSlider] = []
    private var boldSwitch: UISwitch?
    private let selectionFeedback = UISelectionFeedbackGenerator()

    init(
        theme: ReadingTheme,
        textSize: ReadingTextSize,
        style: ReaderUserStyle,
        fallbackPhrase: String,
        phraseProvider: @escaping () async -> String?
    ) {
        self.theme = theme
        self.textSize = textSize
        self.style = style
        self.fallbackPhrase = fallbackPhrase
        self.phraseProvider = phraseProvider
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = InkColor.bgSurface
        selectionFeedback.prepare()

        let scrollView = UIScrollView()
        scrollView.alwaysBounceVertical = true
        scrollView.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(scrollView)

        let stack = UIStackView()
        stack.axis = .vertical
        stack.spacing = InkSpacing.space4
        stack.translatesAutoresizingMaskIntoConstraints = false
        scrollView.addSubview(stack)

        NSLayoutConstraint.activate([
            scrollView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            scrollView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            scrollView.topAnchor.constraint(equalTo: view.topAnchor),
            scrollView.bottomAnchor.constraint(equalTo: view.bottomAnchor),
            stack.leadingAnchor.constraint(equalTo: scrollView.contentLayoutGuide.leadingAnchor, constant: InkSpacing.pageMargin),
            stack.trailingAnchor.constraint(equalTo: scrollView.contentLayoutGuide.trailingAnchor, constant: -InkSpacing.pageMargin),
            stack.topAnchor.constraint(equalTo: scrollView.contentLayoutGuide.topAnchor, constant: InkSpacing.space5),
            stack.bottomAnchor.constraint(equalTo: scrollView.contentLayoutGuide.bottomAnchor, constant: -InkSpacing.space6),
            stack.widthAnchor.constraint(equalTo: scrollView.frameLayoutGuide.widthAnchor, constant: -2 * InkSpacing.pageMargin),
        ])

        buildHeader(into: stack)
        buildPreview(into: stack)
        buildTypeSection(into: stack)
        buildLayoutSection(into: stack)
        buildReset(into: stack)

        fetchPhrase()
    }

    // MARK: Sections

    private func buildHeader(into stack: UIStackView) {
        let back = InkIconButton(
            symbol: "chevron.backward",
            accessibilityLabel: String(localized: "a11y_back", defaultValue: "Back")
        ) { [weak self] in
            self?.navigationController?.popViewController(animated: true)
        }

        let title = InkLabel()
        title.text = String(localized: "reader_customize_title", defaultValue: "Customize")
        title.font = InkFont.heading
        title.textColor = InkColor.textDisplay
        title.accessibilityTraits = .header

        let close = InkCloseButton { [weak self] in self?.onClose?() }

        let row = UIStackView(arrangedSubviews: [back, title, UIView(), close])
        row.axis = .horizontal
        row.alignment = .center
        row.spacing = InkSpacing.space3
        stack.addArrangedSubview(row)
        stack.addArrangedSubview(hairline())
        stack.setCustomSpacing(InkSpacing.space3, after: row)
    }

    private func buildPreview(into stack: UIStackView) {
        let card = ReaderPreviewCard(
            theme: theme,
            textSize: textSize,
            phrase: fallbackPhrase,
            style: style
        )
        previewCard = card
        stack.addArrangedSubview(card)
        stack.setCustomSpacing(InkSpacing.space5, after: card)
    }

    private func buildTypeSection(into stack: UIStackView) {
        stack.addArrangedSubview(eyebrow(String(localized: "reader_type_section", defaultValue: "Type")))

        // Font: a value row that is its own pull-down — UIKit's menu is
        // the platform picker, so the roster no longer pushes the panel
        // around and its items are natively focusable for VoiceOver.
        let valueLabel = InkLabel()
        valueLabel.text = style.font.displayName
        valueLabel.font = InkFont.caption
        valueLabel.textColor = InkColor.textTertiary
        fontValueLabel = valueLabel

        let chevron = UIImageView(
            image: UIImage(
                systemName: "chevron.up.chevron.down",
                withConfiguration: UIImage.SymbolConfiguration(pointSize: 13, weight: .semibold)
            )
        )
        chevron.tintColor = InkColor.textTertiary
        chevron.setContentHuggingPriority(.required, for: .horizontal)

        let trailing = UIStackView(arrangedSubviews: [valueLabel, chevron])
        trailing.axis = .horizontal
        trailing.spacing = InkSpacing.space2
        trailing.alignment = .center

        let fontTitle = String(localized: "reader_font", defaultValue: "Font")
        let fontRow = menuRow(
            symbol: "textformat",
            title: fontTitle,
            trailing: trailing
        )
        fontRow.accessibilityLabel = fontTitle
        self.fontRow = fontRow
        setFontDisplay(style.font)

        // Bold text.
        let boldToggle = UISwitch()
        boldToggle.onTintColor = InkColor.accentFill
        boldToggle.isOn = style.bold
        boldToggle.accessibilityLabel = String(localized: "a11y_bold_text", defaultValue: "Bold text")
        boldToggle.addAction(UIAction { [weak self] action in
            guard let self, let toggle = action.sender as? UISwitch else { return }
            self.selectionFeedback.selectionChanged()
            self.style.bold = toggle.isOn
            self.commitStyle()
        }, for: .valueChanged)
        boldSwitch = boldToggle
        let boldRow = plainRow(
            symbol: "bold",
            title: String(localized: "reader_bold", defaultValue: "Bold text"),
            trailing: boldToggle
        )

        let card = groupCard(rows: [fontRow, boldRow])
        stack.addArrangedSubview(card)
        stack.setCustomSpacing(InkSpacing.space5, after: card)
    }

    /// The five faces, current one checked. Rebuilt on every pick so the
    /// checkmark and the row's readout can never disagree. Per-item
    /// specimens are deliberately absent: `UIMenu` renders plain titles
    /// only, and the preview card already shows the face in use.
    private func fontMenu() -> UIMenu {
        let current = style.font
        let actions = ReadingFont.allCases.map { font in
            UIAction(
                title: font.displayName,
                state: font == current ? .on : .off
            ) { [weak self] _ in
                self?.pickFont(font)
            }
        }
        return UIMenu(children: actions)
    }

    /// Always commits, even for the face already shown: a stored id this
    /// build does not know reads back as `.publisher`, and an explicit
    /// pick is the one thing allowed to overwrite it.
    private func pickFont(_ font: ReadingFont) {
        selectionFeedback.selectionChanged()
        style.font = font
        style.fontID = font.rawValue
        setFontDisplay(font)
        commitStyle()
    }

    private func buildLayoutSection(into stack: UIStackView) {
        stack.addArrangedSubview(eyebrow(String(localized: "reader_layout_section", defaultValue: "Layout")))

        let lineFormatter = NumberFormatter()
        lineFormatter.minimumFractionDigits = 2
        lineFormatter.maximumFractionDigits = 2

        let percentFormatter = NumberFormatter()
        percentFormatter.numberStyle = .percent
        percentFormatter.maximumFractionDigits = 1

        let marginsFormat = NSLocalizedString("reader_margins_value", comment: "")

        let configs: [(InkStepSlider.Config, Double, WritableKeyPath<ReaderUserStyle, Double>?)] = [
            (
                InkStepSlider.Config(
                    range: 1.30 ... 2.10,
                    step: 0.05,
                    label: String(localized: "reader_line_spacing", defaultValue: "Line spacing"),
                    a11yLabel: String(localized: "a11y_line_spacing", defaultValue: "Line spacing"),
                    format: { lineFormatter.string(from: NSNumber(value: $0)) ?? "\($0)" }
                ),
                style.lineSpacing,
                \ReaderUserStyle.lineSpacing
            ),
            (
                InkStepSlider.Config(
                    range: 0 ... 0.06,
                    step: 0.005,
                    label: String(localized: "reader_letter_spacing", defaultValue: "Letter spacing"),
                    a11yLabel: String(localized: "a11y_letter_spacing", defaultValue: "Letter spacing"),
                    format: { percentFormatter.string(from: NSNumber(value: $0)) ?? "\($0)" }
                ),
                style.letterSpacing,
                \ReaderUserStyle.letterSpacing
            ),
            (
                InkStepSlider.Config(
                    range: 0 ... 0.30,
                    step: 0.025,
                    label: String(localized: "reader_word_spacing", defaultValue: "Word spacing"),
                    a11yLabel: String(localized: "a11y_word_spacing", defaultValue: "Word spacing"),
                    format: { percentFormatter.string(from: NSNumber(value: $0)) ?? "\($0)" }
                ),
                style.wordSpacing,
                \ReaderUserStyle.wordSpacing
            ),
            (
                InkStepSlider.Config(
                    range: 16 ... 48,
                    step: 2,
                    label: String(localized: "reader_margins", defaultValue: "Margins"),
                    a11yLabel: String(localized: "a11y_margins", defaultValue: "Page margins"),
                    format: { String.localizedStringWithFormat(marginsFormat, Int($0.rounded())) }
                ),
                Double(style.margins),
                nil // margins is Int; handled in the closure below
            ),
        ]

        for (config, initial, keyPath) in configs {
            let slider = InkStepSlider(config: config, value: initial)
            slider.onTouchDown = { [weak self] in
                guard let self else { return }
                self.onSessionBegin?(self.style)
            }
            slider.onPreview = { [weak self] value in
                guard let self else { return }
                self.update(keyPath, to: value)
                self.previewCard?.apply(self.style)
                self.onPreview?(self.style)
            }
            slider.onCommit = { [weak self] value in
                guard let self else { return }
                self.update(keyPath, to: value)
                self.commitStyle()
            }
            sliders.append(slider)
            stack.addArrangedSubview(slider)
        }
    }

    private func buildReset(into stack: UIStackView) {
        let reset = InkButton(
            String(localized: "reader_reset_defaults", defaultValue: "Reset to defaults"),
            variant: .recessed,
            symbol: "arrow.counterclockwise"
        ) { [weak self] in self?.resetToDefaults() }
        reset.accessibilityLabel = String(
            localized: "a11y_reset_defaults",
            defaultValue: "Reset type and layout to defaults"
        )
        stack.addArrangedSubview(reset)
        stack.setCustomSpacing(InkSpacing.space6, after: sliders.last ?? reset)
    }

    // MARK: Behavior

    private func update(_ keyPath: WritableKeyPath<ReaderUserStyle, Double>?, to value: Double) {
        if let keyPath {
            style[keyPath: keyPath] = value
        } else {
            style.margins = Int(value.rounded())
        }
    }

    /// The Font row's readout, its VoiceOver value, and the menu's checked
    /// item move together — either all change or none do.
    private func setFontDisplay(_ font: ReadingFont) {
        fontValueLabel?.text = font.displayName
        fontRow?.accessibilityValue = font.displayName
        fontRow?.menu = fontMenu()
    }

    /// Persist + apply, and refresh the preview card.
    private func commitStyle() {
        previewCard?.apply(style)
        onCommit?(style)
    }

    private func resetToDefaults() {
        UIImpactFeedbackGenerator(style: .light).impactOccurred()
        style = ReaderUserStyle(
            font: .publisher,
            fontID: ReadingFont.publisher.rawValue,
            bold: false,
            lineSpacing: 1.65,
            letterSpacing: 0,
            wordSpacing: 0,
            margins: 26
        )
        // Rebuild the controls to the fresh values in place.
        boldSwitch?.setOn(false, animated: true)
        setFontDisplay(style.font)
        let values: [Double] = [style.lineSpacing, style.letterSpacing, style.wordSpacing, Double(style.margins)]
        for (slider, value) in zip(sliders, values) {
            slider.setValue(value)
        }
        commitStyle()
    }

    #if DEBUG
    /// Screenshot loop only: `-inkuna.debugReaderUI fontmenu` opens the
    /// Font pull-down without a finger.
    func debugOpenFontMenu() {
        fontRow?.performPrimaryAction()
    }
    #endif

    private func fetchPhrase() {
        Task { [weak self] in
            guard let self else { return }
            guard let phrase = await self.phraseProvider(), !phrase.isEmpty else { return }
            let card = self.previewCard
            if UIAccessibility.isReduceMotionEnabled {
                card?.phrase = phrase
            } else {
                if let card {
                    UIView.transition(with: card, duration: InkMotion.fast, options: .transitionCrossDissolve) {
                        card.phrase = phrase
                    }
                }
            }
        }
    }

    // MARK: Row builders (the Settings grouped-card idiom)

    private func groupCard(rows: [UIView]) -> UIView {
        let stack = UIStackView()
        stack.axis = .vertical
        for (index, row) in rows.enumerated() {
            if index > 0 { stack.addArrangedSubview(hairline()) }
            stack.addArrangedSubview(row)
        }
        stack.backgroundColor = InkColor.bgSurface
        stack.layer.cornerRadius = InkRadius.lg
        stack.clipsToBounds = true
        stack.installInkShadow(.sm)
        return stack
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

    private func eyebrow(_ text: String) -> UIView {
        let label = InkLabel()
        label.attributedText = InkFont.eyebrow(text)
        label.accessibilityTraits = .header
        return label
    }

    private func rowGlyph(_ symbol: String) -> UIImageView {
        let view = UIImageView(
            image: UIImage(
                systemName: symbol,
                withConfiguration: UIImage.SymbolConfiguration(pointSize: 18, weight: .regular)
            )
        )
        view.tintColor = InkColor.textSecondary
        view.setContentHuggingPriority(.required, for: .horizontal)
        view.setContentCompressionResistancePriority(.required, for: .horizontal)
        return view
    }

    private func rowTitle(_ title: String) -> InkLabel {
        let label = InkLabel()
        label.text = title
        label.font = InkFont.ui
        label.textColor = InkColor.textDisplay
        label.numberOfLines = 0
        return label
    }

    private func plainRow(symbol: String, title: String, trailing: UIView) -> UIView {
        let row = UIStackView(arrangedSubviews: [rowGlyph(symbol), rowTitle(title), UIView(), trailing])
        row.axis = .horizontal
        row.alignment = .center
        row.spacing = InkSpacing.space4
        row.isLayoutMarginsRelativeArrangement = true
        row.layoutMargins = UIEdgeInsets(top: 12, left: InkSpacing.space4, bottom: 12, right: InkSpacing.space4)
        return row
    }

    /// A grouped-list row whose whole surface is the pull-down: the menu
    /// is the row's primary action, so one tap opens it right over the
    /// value it replaces.
    private func menuRow(symbol: String, title: String, trailing: UIView) -> MenuRowControl {
        let content = UIStackView(arrangedSubviews: [rowGlyph(symbol), rowTitle(title), UIView(), trailing])
        content.axis = .horizontal
        content.alignment = .center
        content.spacing = InkSpacing.space4
        content.isUserInteractionEnabled = false
        content.translatesAutoresizingMaskIntoConstraints = false

        let control = MenuRowControl()
        control.addSubview(content)
        NSLayoutConstraint.activate([
            content.leadingAnchor.constraint(equalTo: control.leadingAnchor, constant: InkSpacing.space4),
            content.trailingAnchor.constraint(equalTo: control.trailingAnchor, constant: -InkSpacing.space4),
            content.topAnchor.constraint(equalTo: control.topAnchor, constant: 12),
            content.bottomAnchor.constraint(equalTo: control.bottomAnchor, constant: -12),
        ])
        return control
    }
}

/// A grouped-list row that presents its `menu` on a plain tap and washes
/// accent-soft while pressed. One accessibility element: the row reads as
/// "Font, <value>", and the menu's own items are focusable on their own.
final class MenuRowControl: UIButton {
    init() {
        super.init(frame: .zero)
        showsMenuAsPrimaryAction = true
        isAccessibilityElement = true
        // No manual trait override: a `UIButton` whose primary action is a
        // menu supplies its own traits, and pinning `.button` here would
        // flatten whatever pop-up semantics UIKit adds.
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

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
