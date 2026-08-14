import UIKit

/// First-run welcome: the ink-and-moon brand mark, the name, and Begin.
final class WelcomeViewController: UIViewController {
    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = InkColor.bgApp

        let mark = BrandMarkView()

        // TODO(l10n): localize once the strings pass lands.
        let nameLabel = InkLabel()
        nameLabel.text = "Inkuna"
        nameLabel.font = InkFont.serif(54, weight: .light, style: .largeTitle)
        nameLabel.textColor = InkColor.textDisplay

        let taglineLabel = InkLabel()
        taglineLabel.text = "A minimalist book reader where ink meets moonlight."
        taglineLabel.font = InkFont.reading()
        taglineLabel.textColor = InkColor.textSecondary
        taglineLabel.textAlignment = .center
        taglineLabel.numberOfLines = 0

        let beginButton = InkButton("Begin", size: .large) { [weak self] in
            self?.navigationController?.pushViewController(ThemePickViewController(), animated: true)
        }

        let stack = UIStackView(arrangedSubviews: [mark, nameLabel, taglineLabel, beginButton])
        stack.axis = .vertical
        stack.alignment = .center
        stack.spacing = 14
        stack.setCustomSpacing(26, after: mark)
        stack.setCustomSpacing(InkSpacing.space10 + InkSpacing.space1, after: taglineLabel)
        stack.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(stack)

        NSLayoutConstraint.activate([
            stack.centerXAnchor.constraint(equalTo: view.centerXAnchor),
            stack.centerYAnchor.constraint(equalTo: view.centerYAnchor),
            stack.leadingAnchor.constraint(greaterThanOrEqualTo: view.leadingAnchor, constant: 36),
            taglineLabel.widthAnchor.constraint(lessThanOrEqualToConstant: 384),
        ])
    }
}

/// The brand mark: an ink disc with the moon taking a bite of light out
/// of it — drawn from the same two circles as the app icon.
private final class BrandMarkView: UIView {
    private let disc = UIView()
    private let moon = UIView()

    init() {
        super.init(frame: .zero)
        disc.backgroundColor = InkColor.textDisplay.withAlphaComponent(0.9)
        disc.layer.cornerRadius = 22
        moon.backgroundColor = InkColor.bgApp
        moon.layer.cornerRadius = 22

        for circle in [disc, moon] {
            circle.translatesAutoresizingMaskIntoConstraints = false
            addSubview(circle)
        }
        NSLayoutConstraint.activate([
            widthAnchor.constraint(equalToConstant: 44),
            heightAnchor.constraint(equalToConstant: 44),
            disc.leadingAnchor.constraint(equalTo: leadingAnchor),
            disc.topAnchor.constraint(equalTo: topAnchor),
            disc.widthAnchor.constraint(equalToConstant: 44),
            disc.heightAnchor.constraint(equalToConstant: 44),
            moon.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 11),
            moon.topAnchor.constraint(equalTo: topAnchor, constant: -3),
            moon.widthAnchor.constraint(equalToConstant: 44),
            moon.heightAnchor.constraint(equalToConstant: 44),
        ])
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }
}
