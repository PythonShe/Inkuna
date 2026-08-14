import UIKit

/// Hairline reading-progress bar (design-system `ProgressBar`):
/// a 3pt recessed track with an amber fill that settles on the page curve.
final class InkProgressBar: UIView {
    static let barHeight: CGFloat = 3

    private let fill = UIView()
    private var fillWidth: NSLayoutConstraint?
    private(set) var progress: CGFloat = 0

    init(progress: CGFloat = 0) {
        super.init(frame: .zero)
        backgroundColor = InkColor.bgRecessed
        layer.cornerRadius = Self.barHeight / 2
        layer.masksToBounds = true

        fill.backgroundColor = InkColor.accent
        fill.layer.cornerRadius = Self.barHeight / 2
        fill.translatesAutoresizingMaskIntoConstraints = false
        addSubview(fill)
        NSLayoutConstraint.activate([
            fill.leadingAnchor.constraint(equalTo: leadingAnchor),
            fill.topAnchor.constraint(equalTo: topAnchor),
            fill.bottomAnchor.constraint(equalTo: bottomAnchor),
            heightAnchor.constraint(equalToConstant: Self.barHeight),
        ])
        setProgress(progress, animated: false)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    /// - Parameter progress: 0…1 fraction of the book read.
    func setProgress(_ progress: CGFloat, animated: Bool = true) {
        self.progress = min(max(progress, 0), 1)
        fillWidth?.isActive = false
        let constraint = fill.widthAnchor.constraint(equalTo: widthAnchor, multiplier: self.progress)
        constraint.isActive = true
        fillWidth = constraint
        guard animated, window != nil else { return }
        let animator = InkMotion.pageAnimator(duration: InkMotion.slow)
        animator.addAnimations { self.layoutIfNeeded() }
        animator.startAnimation()
    }
}
