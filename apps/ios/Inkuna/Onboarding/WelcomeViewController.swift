import UIKit

/// First-run welcome: the ink-and-moon brand mark, the name, and Begin.
final class WelcomeViewController: UIViewController {
    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = InkColor.bgApp

        let mark = BrandMarkView()

        let nameLabel = InkLabel()
        nameLabel.text = String(localized: "app_name", defaultValue: "Inkuna")
        nameLabel.font = InkFont.serif(54, weight: .light, style: .largeTitle)
        nameLabel.textColor = InkColor.textDisplay

        let taglineLabel = InkLabel()
        taglineLabel.text = String(localized: "welcome_tagline", defaultValue: "A minimalist book reader where ink meets moonlight.")
        taglineLabel.font = InkFont.reading()
        taglineLabel.textColor = InkColor.textSecondary
        taglineLabel.textAlignment = .center
        taglineLabel.numberOfLines = 0

        let beginButton = InkButton(String(localized: "welcome_begin", defaultValue: "Begin"), size: .large) { [weak self] in
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

/// The canonical Inkuna mark (`assets/brand/inkuna-mark.svg`): an ink disc
/// with a paper moon floating inside, tangent to the rim at the upper
/// right — scaled from the 1024 canvas to 44pt and drawn in theme ink so
/// it reads in both day and night.
private final class BrandMarkView: UIView {
    /// Source canvas: disc r=430 at (512,512), moon r=266 at (676,389).
    private static let discSize: CGFloat = 44
    private static let scale = discSize / 860
    private static let moonSize = 532 * scale
    private static let moonX = (410 - 82) * scale
    private static let moonY = (123 - 82) * scale

    private let disc = UIView()
    private let moon = UIView()

    init() {
        super.init(frame: .zero)
        disc.backgroundColor = InkColor.textDisplay.withAlphaComponent(0.9)
        disc.layer.cornerRadius = Self.discSize / 2
        moon.backgroundColor = InkColor.bgApp
        moon.layer.cornerRadius = Self.moonSize / 2

        for circle in [disc, moon] {
            circle.translatesAutoresizingMaskIntoConstraints = false
            addSubview(circle)
        }
        NSLayoutConstraint.activate([
            widthAnchor.constraint(equalToConstant: Self.discSize),
            heightAnchor.constraint(equalToConstant: Self.discSize),
            disc.leadingAnchor.constraint(equalTo: leadingAnchor),
            disc.topAnchor.constraint(equalTo: topAnchor),
            disc.widthAnchor.constraint(equalToConstant: Self.discSize),
            disc.heightAnchor.constraint(equalToConstant: Self.discSize),
            moon.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Self.moonX),
            moon.topAnchor.constraint(equalTo: topAnchor, constant: Self.moonY),
            moon.widthAnchor.constraint(equalToConstant: Self.moonSize),
            moon.heightAnchor.constraint(equalToConstant: Self.moonSize),
        ])
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }
}
