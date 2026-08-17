import SafariServices
import UIKit

/// The Account sheet: the purely local profile, preferences (evening
/// reminder, night mode), account editing, and About — presented as a
/// native detent sheet from the Tonight header's avatar button.
///
/// The design's "App updates" row is Android-only; iOS updates ship
/// through the App Store/TestFlight, so it does not appear here.
final class SettingsViewController: UIViewController {
    private let scrollView = UIScrollView()
    private let contentStack = UIStackView()

    private let avatarInitial = InkLabel()
    private let avatarGlyph = UIImageView()
    private let nameLabel = InkLabel()
    private let emailLabel = InkLabel()

    private let reminderSwitch = UISwitch()
    private let nightSwitch = UISwitch()
    private let selectionFeedback = UISelectionFeedbackGenerator()

    init() {
        super.init(nibName: nil, bundle: nil)
        modalPresentationStyle = .pageSheet
        if let sheet = sheetPresentationController {
            sheet.detents = [.large()]
            sheet.prefersGrabberVisible = true
            sheet.preferredCornerRadius = InkRadius.xl
        }
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = InkColor.bgApp
        selectionFeedback.prepare()

        scrollView.alwaysBounceVertical = true
        scrollView.showsVerticalScrollIndicator = false
        scrollView.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(scrollView)

        contentStack.axis = .vertical
        contentStack.translatesAutoresizingMaskIntoConstraints = false
        scrollView.addSubview(contentStack)

        NSLayoutConstraint.activate([
            scrollView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            scrollView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            scrollView.topAnchor.constraint(equalTo: view.topAnchor),
            scrollView.bottomAnchor.constraint(equalTo: view.bottomAnchor),
            contentStack.leadingAnchor.constraint(equalTo: scrollView.contentLayoutGuide.leadingAnchor, constant: InkSpacing.pageMargin),
            contentStack.trailingAnchor.constraint(equalTo: scrollView.contentLayoutGuide.trailingAnchor, constant: -InkSpacing.pageMargin),
            contentStack.topAnchor.constraint(equalTo: scrollView.contentLayoutGuide.topAnchor, constant: InkSpacing.space6),
            contentStack.bottomAnchor.constraint(equalTo: scrollView.contentLayoutGuide.bottomAnchor, constant: -InkSpacing.space10),
            contentStack.widthAnchor.constraint(equalTo: scrollView.frameLayoutGuide.widthAnchor, constant: -2 * InkSpacing.pageMargin),
        ])

        buildHeader()
        buildProfileCard()
        buildPreferences()
        buildAccountGroup()
        buildAboutGroup()
        buildFooter()
        refreshProfile()
    }

    // MARK: Sections

    private func buildHeader() {
        let titleLabel = InkLabel()
        titleLabel.text = String(localized: "settings_title", defaultValue: "Account")
        titleLabel.font = InkFont.serif(27, weight: .light, style: .title1)
        titleLabel.textColor = InkColor.textDisplay

        let closeButton = InkCloseButton { [weak self] in self?.dismiss(animated: true) }
        let row = UIStackView(arrangedSubviews: [titleLabel, UIView(), closeButton])
        row.axis = .horizontal
        row.alignment = .center
        contentStack.addArrangedSubview(row)
        contentStack.setCustomSpacing(InkSpacing.space5, after: row)
    }

    private func buildProfileCard() {
        let avatar = UIView()
        avatar.backgroundColor = InkColor.bgRecessed
        avatar.layer.cornerRadius = 26
        avatar.clipsToBounds = true
        avatar.translatesAutoresizingMaskIntoConstraints = false

        avatarInitial.font = InkFont.serif(22, weight: .medium, style: .title2)
        avatarInitial.textColor = InkColor.textSecondary
        avatarInitial.translatesAutoresizingMaskIntoConstraints = false
        avatar.addSubview(avatarInitial)

        avatarGlyph.image = UIImage(
            systemName: "person.fill",
            withConfiguration: UIImage.SymbolConfiguration(pointSize: 22, weight: .regular)
        )
        avatarGlyph.tintColor = InkColor.textTertiary
        avatarGlyph.translatesAutoresizingMaskIntoConstraints = false
        avatar.addSubview(avatarGlyph)

        nameLabel.font = InkFont.heading
        nameLabel.textColor = InkColor.textDisplay

        emailLabel.font = InkFont.caption
        emailLabel.textColor = InkColor.textTertiary

        let text = UIStackView(arrangedSubviews: [nameLabel, emailLabel])
        text.axis = .vertical
        text.spacing = 2

        let card = UIStackView(arrangedSubviews: [avatar, text])
        card.axis = .horizontal
        card.alignment = .center
        card.spacing = InkSpacing.space4
        card.isLayoutMarginsRelativeArrangement = true
        card.layoutMargins = UIEdgeInsets(top: InkSpacing.space4, left: InkSpacing.space4, bottom: InkSpacing.space4, right: InkSpacing.space4)
        card.backgroundColor = InkColor.bgSurface
        card.layer.cornerRadius = InkRadius.lg
        card.installInkShadow(.sm)

        NSLayoutConstraint.activate([
            avatar.widthAnchor.constraint(equalToConstant: 52),
            avatar.heightAnchor.constraint(equalToConstant: 52),
            avatarInitial.centerXAnchor.constraint(equalTo: avatar.centerXAnchor),
            avatarInitial.centerYAnchor.constraint(equalTo: avatar.centerYAnchor),
            avatarGlyph.centerXAnchor.constraint(equalTo: avatar.centerXAnchor),
            avatarGlyph.centerYAnchor.constraint(equalTo: avatar.centerYAnchor),
        ])

        contentStack.addArrangedSubview(card)
        contentStack.setCustomSpacing(InkSpacing.space6, after: card)
    }

    private func buildPreferences() {
        let eyebrow = InkLabel()
        eyebrow.attributedText = InkFont.eyebrow(String(localized: "settings_preferences", defaultValue: "Preferences"))
        contentStack.addArrangedSubview(eyebrow)
        contentStack.setCustomSpacing(10, after: eyebrow)

        let settings = AppSettings.shared

        reminderSwitch.onTintColor = InkColor.accentFill
        reminderSwitch.isOn = settings.eveningReminder
        reminderSwitch.addAction(UIAction { [weak self] _ in self?.reminderToggled() }, for: .valueChanged)

        nightSwitch.onTintColor = InkColor.accentFill
        nightSwitch.isOn = settings.readingTheme.isNight
        nightSwitch.addAction(UIAction { [weak self] _ in self?.nightToggled() }, for: .valueChanged)

        // TODO(accounts): the design's "Redeem a code" row needs a
        // redemption backend; the account is purely local today.
        let group = groupCard(rows: [
            settingRow(
                symbol: "bell",
                title: String(localized: "settings_reminder", defaultValue: "Evening reminder"),
                subtitle: String(localized: "settings_reminder_sub", defaultValue: "A quiet nudge at your reading hour"),
                trailing: reminderSwitch
            ),
            settingRow(
                symbol: "moon",
                title: String(localized: "settings_night", defaultValue: "Night mode"),
                subtitle: ReadingTheme.moon.subtitle,
                trailing: nightSwitch
            ),
        ])
        contentStack.addArrangedSubview(group)
        contentStack.setCustomSpacing(InkSpacing.space6, after: group)
    }

    private func buildAccountGroup() {
        // TODO(accounts): the design's "Sign out" row waits on real
        // accounts; there is nothing to sign out of on a local one.
        let group = groupCard(rows: [
            disclosureRow(
                symbol: "person.text.rectangle",
                title: String(localized: "settings_account_settings", defaultValue: "Account settings")
            ) { [weak self] in self?.presentAccountEditor() },
        ])
        contentStack.addArrangedSubview(group)
        contentStack.setCustomSpacing(InkSpacing.space6, after: group)
    }

    private func buildAboutGroup() {
        let group = groupCard(rows: [
            disclosureRow(
                symbol: "info.circle",
                title: String(localized: "settings_about", defaultValue: "About Inkuna")
            ) { [weak self] in self?.openAbout() },
        ])
        contentStack.addArrangedSubview(group)
        contentStack.setCustomSpacing(InkSpacing.space8, after: group)
    }

    private func buildFooter() {
        let version = Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "1.0"
        let footer = InkLabel()
        let format = NSLocalizedString("settings_footer", comment: "")
        footer.text = String.localizedStringWithFormat(format, version)
        footer.font = InkFont.caption
        footer.textColor = InkColor.textTertiary
        footer.textAlignment = .center
        footer.numberOfLines = 0
        contentStack.addArrangedSubview(footer)
    }

    // MARK: Actions

    private func reminderToggled() {
        if reminderSwitch.isOn {
            Task { @MainActor [weak self] in
                guard let self else { return }
                if await EveningReminder.enable() {
                    AppSettings.shared.eveningReminder = true
                } else {
                    // Permission declined: the choice cannot take effect.
                    self.reminderSwitch.setOn(false, animated: true)
                    AppSettings.shared.eveningReminder = false
                }
            }
        } else {
            EveningReminder.disable()
            AppSettings.shared.eveningReminder = false
        }
        selectionFeedback.selectionChanged()
    }

    /// The theme flip queued behind the switch's own thumb animation;
    /// cancelled and re-queued when the switch is flipped again first.
    private var pendingNightFlip: DispatchWorkItem?

    private func nightToggled() {
        selectionFeedback.selectionChanged()
        let night = nightSwitch.isOn
        // The canonical day/night pair from onboarding: the switch flips
        // between Paper and Moon, exactly like the design's toggle. The
        // flip waits for the thumb to land: the window cross-fade
        // snapshots the screen, and a switch caught mid-animation reads
        // as an uncomfortable blink.
        pendingNightFlip?.cancel()
        let flip = DispatchWorkItem {
            AppSettings.shared.readingTheme = night ? .moon : .paper
        }
        pendingNightFlip = flip
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.25, execute: flip)
    }

    private func presentAccountEditor() {
        let alert = UIAlertController(
            title: String(localized: "settings_account_settings", defaultValue: "Account settings"),
            message: nil,
            preferredStyle: .alert
        )
        alert.addTextField { field in
            field.placeholder = String(localized: "settings_edit_name", defaultValue: "Username")
            field.text = AppSettings.shared.accountName
            field.autocapitalizationType = .words
        }
        alert.addTextField { field in
            field.placeholder = String(localized: "settings_edit_email", defaultValue: "Email")
            field.text = AppSettings.shared.accountEmail
            field.keyboardType = .emailAddress
            field.autocapitalizationType = .none
            field.autocorrectionType = .no
        }
        alert.addAction(UIAlertAction(title: String(localized: "settings_cancel", defaultValue: "Cancel"), style: .cancel))
        alert.addAction(UIAlertAction(title: String(localized: "settings_save", defaultValue: "Save"), style: .default) { [weak self, weak alert] _ in
            guard let self, let fields = alert?.textFields else { return }
            let name = fields[0].text?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            let email = fields[1].text?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            AppSettings.shared.accountName = name
            AppSettings.shared.accountEmail = email
            self.refreshProfile()
        })
        present(alert, animated: true)
    }

    private func openAbout() {
        guard let url = URL(string: "https://inkuna.app") else { return }
        present(SFSafariViewController(url: url), animated: true)
    }

    private func refreshProfile() {
        let settings = AppSettings.shared
        let name = settings.accountName
        nameLabel.text = name.isEmpty ? String(localized: "settings_default_name", defaultValue: "Reader") : name
        emailLabel.text = settings.accountEmail.isEmpty
            ? String(localized: "settings_local_account", defaultValue: "On this device")
            : settings.accountEmail
        if let first = name.first {
            avatarInitial.text = String(first).uppercased()
            avatarInitial.isHidden = false
            avatarGlyph.isHidden = true
        } else {
            avatarInitial.isHidden = true
            avatarGlyph.isHidden = false
        }
    }

    // MARK: Row building

    /// A grouped surface card with hairline separators between rows.
    private func groupCard(rows: [UIView]) -> UIView {
        let stack = UIStackView()
        stack.axis = .vertical
        for (index, row) in rows.enumerated() {
            if index > 0 {
                let separator = UIView()
                separator.backgroundColor = InkColor.borderHairline
                let inset = UIStackView(arrangedSubviews: [separator])
                inset.isLayoutMarginsRelativeArrangement = true
                inset.layoutMargins = UIEdgeInsets(top: 0, left: InkSpacing.space4, bottom: 0, right: 0)
                separator.heightAnchor.constraint(equalToConstant: 1 / max(traitCollection.displayScale, 1)).isActive = true
                stack.addArrangedSubview(inset)
            }
            stack.addArrangedSubview(row)
        }
        stack.backgroundColor = InkColor.bgSurface
        stack.layer.cornerRadius = InkRadius.lg
        stack.clipsToBounds = true
        stack.installInkShadow(.sm)
        return stack
    }

    /// Leading symbol, title (+ optional subtitle), and a trailing control.
    private func settingRow(symbol: String, title: String, subtitle: String? = nil, trailing: UIView) -> UIView {
        let row = UIStackView(arrangedSubviews: [rowGlyph(symbol), rowText(title: title, subtitle: subtitle), trailing])
        row.axis = .horizontal
        row.alignment = .center
        row.spacing = InkSpacing.space4
        row.isLayoutMarginsRelativeArrangement = true
        row.layoutMargins = UIEdgeInsets(top: 14, left: InkSpacing.space4, bottom: 14, right: InkSpacing.space4)
        return row
    }

    /// A tappable row with a trailing chevron.
    private func disclosureRow(symbol: String, title: String, onTap: @escaping @MainActor () -> Void) -> UIView {
        let chevron = UIImageView(
            image: UIImage(
                systemName: "chevron.right",
                withConfiguration: UIImage.SymbolConfiguration(pointSize: 13, weight: .semibold)
            )
        )
        chevron.tintColor = InkColor.textTertiary
        chevron.setContentHuggingPriority(.required, for: .horizontal)

        let content = UIStackView(arrangedSubviews: [rowGlyph(symbol), rowText(title: title, subtitle: nil), chevron])
        content.axis = .horizontal
        content.alignment = .center
        content.spacing = InkSpacing.space4
        content.isUserInteractionEnabled = false
        content.translatesAutoresizingMaskIntoConstraints = false

        let control = SettingsRowControl(onTap: onTap)
        control.accessibilityLabel = title
        control.addSubview(content)
        NSLayoutConstraint.activate([
            content.leadingAnchor.constraint(equalTo: control.leadingAnchor, constant: InkSpacing.space4),
            content.trailingAnchor.constraint(equalTo: control.trailingAnchor, constant: -InkSpacing.space4),
            content.topAnchor.constraint(equalTo: control.topAnchor, constant: 14),
            content.bottomAnchor.constraint(equalTo: control.bottomAnchor, constant: -14),
        ])
        return control
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

    private func rowText(title: String, subtitle: String?) -> UIView {
        let titleLabel = InkLabel()
        titleLabel.text = title
        titleLabel.font = InkFont.ui
        titleLabel.textColor = InkColor.textDisplay
        titleLabel.numberOfLines = 0

        let column = UIStackView(arrangedSubviews: [titleLabel])
        column.axis = .vertical
        column.spacing = 2
        if let subtitle {
            let subtitleLabel = InkLabel()
            subtitleLabel.text = subtitle
            subtitleLabel.font = InkFont.caption
            subtitleLabel.textColor = InkColor.textTertiary
            subtitleLabel.numberOfLines = 0
            column.addArrangedSubview(subtitleLabel)
        }
        return column
    }
}

/// A grouped-list row that washes accent-soft while pressed.
private final class SettingsRowControl: UIControl {
    private let onTap: @MainActor () -> Void

    init(onTap: @escaping @MainActor () -> Void) {
        self.onTap = onTap
        super.init(frame: .zero)
        isAccessibilityElement = true
        accessibilityTraits = .button
        addAction(UIAction { [weak self] _ in self?.onTap() }, for: .touchUpInside)
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
