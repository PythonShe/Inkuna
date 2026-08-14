import UIKit

/// Onboarding's one small ritual: choose Paper or Moon. The choice applies
/// immediately — the whole screen cross-fades into the picked side.
final class ThemePickViewController: UIViewController {
    private var cards: [ThemePreviewCard] = []
    private let selectionFeedback = UISelectionFeedbackGenerator()

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = InkColor.bgApp

        // TODO(l10n): localize once the strings pass lands.
        let eyebrow = UILabel()
        eyebrow.attributedText = InkFont.eyebrow("One small ritual")

        let title = UILabel()
        title.text = "Read by day, or by night"
        title.font = InkFont.display
        title.textColor = InkColor.textDisplay
        title.numberOfLines = 0

        let paperCard = ThemePreviewCard(theme: .paper)
        let moonCard = ThemePreviewCard(theme: .moon)
        cards = [paperCard, moonCard]
        for card in cards {
            card.onPick = { [weak self] theme in self?.pick(theme) }
        }
        refreshSelection()

        let cardRow = UIStackView(arrangedSubviews: cards)
        cardRow.axis = .horizontal
        cardRow.spacing = 14
        cardRow.distribution = .fillEqually
        cardRow.alignment = .top

        let hint = UILabel()
        hint.text = "You can change this anytime while reading."
        hint.font = InkFont.caption
        hint.textColor = InkColor.textTertiary

        let continueButton = InkButton("Continue", size: .large) { [weak self] in
            self?.finishOnboarding()
        }
        let buttonRow = UIStackView(arrangedSubviews: [continueButton])
        buttonRow.axis = .vertical
        buttonRow.alignment = .center

        let stack = UIStackView(arrangedSubviews: [eyebrow, title, cardRow, hint])
        stack.axis = .vertical
        stack.spacing = InkSpacing.space4
        stack.setCustomSpacing(6, after: eyebrow)
        stack.setCustomSpacing(28, after: title)
        stack.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(stack)

        buttonRow.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(buttonRow)

        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: InkSpacing.space6),
            stack.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -InkSpacing.space6),
            stack.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: InkSpacing.space10),
            buttonRow.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            buttonRow.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            buttonRow.bottomAnchor.constraint(equalTo: view.safeAreaLayoutGuide.bottomAnchor, constant: -InkSpacing.space4),
        ])
    }

    private func pick(_ theme: ReadingTheme) {
        selectionFeedback.selectionChanged()
        AppSettings.shared.readingTheme = theme
        refreshSelection()
    }

    private func refreshSelection() {
        let night = AppSettings.shared.readingTheme.isNight
        for card in cards {
            card.isChosen = card.theme.isNight == night
        }
    }

    private func finishOnboarding() {
        AppSettings.shared.hasCompletedOnboarding = true
        guard let window = view.window else { return }
        UIView.transition(with: window, duration: InkMotion.slow, options: .transitionCrossDissolve) {
            window.rootViewController = RootNavigationController(rootViewController: MainViewController())
        }
    }
}

/// A day/night preview card: a few lines of the book set in the theme's
/// own page colors, with the theme's name beneath.
private final class ThemePreviewCard: UIControl {
    let theme: ReadingTheme

    var onPick: ((ReadingTheme) -> Void)?

    var isChosen: Bool = false {
        didSet { restyle() }
    }

    init(theme: ReadingTheme) {
        self.theme = theme
        super.init(frame: .zero)

        backgroundColor = theme.background
        layer.cornerRadius = InkRadius.lg
        layer.borderWidth = 3

        let sample = UILabel()
        sample.text = "The lamp burned low, and the moon took over the work of lighting the page."
        sample.font = InkFont.serif(13, weight: .regular, style: .footnote)
        sample.textColor = theme.foreground
        sample.numberOfLines = 0

        let name = UILabel()
        name.text = theme.displayName
        name.font = InkFont.ui
        name.textColor = theme.foreground

        let subtitle = UILabel()
        subtitle.text = theme.subtitle
        subtitle.font = InkFont.caption
        subtitle.textColor = theme.foreground.withAlphaComponent(0.62)

        let stack = UIStackView(arrangedSubviews: [sample, name, subtitle])
        stack.axis = .vertical
        stack.spacing = 2
        stack.setCustomSpacing(14, after: sample)
        stack.isUserInteractionEnabled = false
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: leadingAnchor, constant: InkSpacing.space4),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -InkSpacing.space4),
            stack.topAnchor.constraint(equalTo: topAnchor, constant: 18),
            stack.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -InkSpacing.space4),
        ])

        accessibilityLabel = "\(theme.displayName). \(theme.subtitle)"
        accessibilityTraits = .button

        registerForTraitChanges([UITraitUserInterfaceStyle.self]) { (card: ThemePreviewCard, _) in
            card.restyle()
        }
        addAction(UIAction { [weak self] _ in
            guard let self else { return }
            self.onPick?(self.theme)
        }, for: .primaryActionTriggered)

        restyle()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    private func restyle() {
        let accent = InkColor.accent.resolvedColor(with: traitCollection)
        layer.borderColor = isChosen ? accent.withAlphaComponent(0.35).cgColor : UIColor.clear.cgColor
        (isChosen ? InkShadow.md : InkShadow.sm).apply(to: layer, style: traitCollection.userInterfaceStyle)
    }
}
