import UIKit

/// What a reader sees before their first book: an invitation, not an
/// apology.
///
/// The stack of three spines is drawn rather than illustrated so it takes
/// its colors from the design tokens and stays right in both day and night
/// without a second asset.
final class EmptyLibraryView: UIView {
    private let onAddBooks: @MainActor () -> Void

    init(onAddBooks: @escaping @MainActor () -> Void) {
        self.onAddBooks = onAddBooks
        super.init(frame: .zero)
        build()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    /// Fades and lifts the state into place — used when the library turns
    /// out to be empty after a load, so it arrives rather than blinks.
    func appear() {
        alpha = 0
        transform = CGAffineTransform(translationX: 0, y: 8)
        let animator = InkMotion.pageAnimator(duration: InkMotion.slow)
        animator.addAnimations {
            self.alpha = 1
            self.transform = .identity
        }
        animator.startAnimation()
    }

    /// The empty state already wired to the import flow — what a library
    /// screen wants, in one line.
    static func inviting(_ presenter: UIViewController) -> EmptyLibraryView {
        EmptyLibraryView { [weak presenter] in
            guard let presenter else { return }
            ImportFlow.presentPicker(from: presenter)
        }
    }

    private func build() {
        // The illustration is a fixed size, so it lives in a container: a
        // `.fill` stack pins every arranged subview edge-to-edge, and a
        // fixed width in that position would drag the whole column down to
        // the illustration's width.
        let spines = UIView()
        let illustration = SpineStackView()
        illustration.translatesAutoresizingMaskIntoConstraints = false
        spines.addSubview(illustration)
        NSLayoutConstraint.activate([
            illustration.widthAnchor.constraint(equalToConstant: 92),
            illustration.heightAnchor.constraint(equalToConstant: 68),
            illustration.centerXAnchor.constraint(equalTo: spines.centerXAnchor),
            illustration.topAnchor.constraint(equalTo: spines.topAnchor),
            illustration.bottomAnchor.constraint(equalTo: spines.bottomAnchor),
        ])

        let title = InkLabel()
        title.text = ImportCopy.emptyTitle
        title.font = InkFont.sectionTitle
        title.textColor = InkColor.textDisplay
        title.textAlignment = .center
        title.numberOfLines = 0

        let body = InkLabel()
        body.text = ImportCopy.emptyBody
        body.font = InkFont.labelRegular
        body.textColor = InkColor.textSecondary
        body.textAlignment = .center
        body.numberOfLines = 0

        let button = InkButton(
            ImportCopy.addBooks,
            variant: .primary,
            size: .large,
            symbol: "plus"
        ) { [weak self] in
            self?.onAddBooks()
        }
        // A container rather than a stack with spacers: a pill button must
        // keep its natural width, and a stack would happily compress it
        // until the title wrapped one letter per line.
        let buttonRow = UIView()
        button.translatesAutoresizingMaskIntoConstraints = false
        buttonRow.addSubview(button)
        NSLayoutConstraint.activate([
            button.centerXAnchor.constraint(equalTo: buttonRow.centerXAnchor),
            button.topAnchor.constraint(equalTo: buttonRow.topAnchor),
            button.bottomAnchor.constraint(equalTo: buttonRow.bottomAnchor),
            button.leadingAnchor.constraint(greaterThanOrEqualTo: buttonRow.leadingAnchor),
        ])

        let footnote = InkLabel()
        footnote.attributedText = InkFont.eyebrow(ImportCopy.emptyFootnote, color: InkColor.textTertiary)
        footnote.textAlignment = .center
        footnote.numberOfLines = 0

        let stack = UIStackView(arrangedSubviews: [spines, title, body, buttonRow, footnote])
        stack.axis = .vertical
        stack.alignment = .fill
        stack.spacing = InkSpacing.space3
        stack.setCustomSpacing(InkSpacing.space6, after: spines)
        stack.setCustomSpacing(InkSpacing.space6, after: body)
        stack.setCustomSpacing(InkSpacing.space5, after: buttonRow)
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)

        // The column wants the full width and settles for the measure —
        // the design system's reading measure halved, because an invitation
        // reads as a caption, not a paragraph of book text. The preferred
        // width has to be stated: with only a maximum, Auto Layout takes
        // the narrowest width the labels can wrap into, which is one word.
        let preferredWidth = stack.widthAnchor.constraint(equalTo: widthAnchor)
        preferredWidth.priority = .defaultHigh
        NSLayoutConstraint.activate([
            preferredWidth,
            stack.centerXAnchor.constraint(equalTo: centerXAnchor),
            stack.topAnchor.constraint(equalTo: topAnchor, constant: InkSpacing.space12),
            stack.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -InkSpacing.space12),
            stack.leadingAnchor.constraint(greaterThanOrEqualTo: leadingAnchor),
            stack.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor),
            stack.widthAnchor.constraint(lessThanOrEqualToConstant: InkFont.readingMeasure / 2),
        ])
    }
}

/// Three empty spines leaning on a shelf line — the shape of a library
/// with nothing in it.
private final class SpineStackView: UIView {
    override init(frame: CGRect) {
        super.init(frame: frame)
        backgroundColor = .clear
        isAccessibilityElement = false
        // Redraw when day/night flips: the strokes are token colors.
        registerForTraitChanges([UITraitUserInterfaceStyle.self]) { (view: UIView, _) in
            view.setNeedsDisplay()
        }
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    override func draw(_ rect: CGRect) {
        guard let context = UIGraphicsGetCurrentContext() else { return }
        let shelfY = rect.maxY - 1
        let heights: [CGFloat] = [0.78, 1.0, 0.62]
        let width: CGFloat = 20
        let gap: CGFloat = 8
        let totalWidth = CGFloat(heights.count) * width + CGFloat(heights.count - 1) * gap
        var x = rect.midX - totalWidth / 2

        context.setLineWidth(1)
        for (index, ratio) in heights.enumerated() {
            let height = (rect.height - 6) * ratio
            let spine = CGRect(x: x, y: shelfY - height, width: width, height: height)
            let path = UIBezierPath(roundedRect: spine, cornerRadius: InkRadius.xs)
            // The middle spine carries the accent wash; the outer two are
            // outlines, so the group reads as "room for more".
            if index == 1 {
                InkColor.accentSoft.setFill()
                path.fill()
                InkColor.accent.withAlphaComponent(0.45).setStroke()
            } else {
                InkColor.borderHairline.setStroke()
            }
            path.lineWidth = 1
            path.stroke()
            x += width + gap
        }

        InkColor.borderHairline.setStroke()
        let shelf = UIBezierPath()
        shelf.move(to: CGPoint(x: rect.midX - totalWidth / 2 - 8, y: shelfY))
        shelf.addLine(to: CGPoint(x: rect.midX + totalWidth / 2 + 8, y: shelfY))
        shelf.lineWidth = 1
        shelf.stroke()
    }
}
