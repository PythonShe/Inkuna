import UIKit

/// A labeled, discretely-stepped slider row: title left, live value
/// readout right, native `UISlider` below. Snaps to the step grid with a
/// selection tick per detent, previews continuously while dragging, and
/// commits once on release — so a drag never floods whoever listens.
final class InkStepSlider: UIControl {
    struct Config {
        let range: ClosedRange<Double>
        let step: Double
        let label: String
        let a11yLabel: String
        /// Renders a value into the readout (and VoiceOver value).
        let format: (Double) -> String
    }

    /// Fires on every step crossing while dragging — preview only.
    var onPreview: ((Double) -> Void)?
    /// Fires once on release, and on every VoiceOver adjustment.
    var onCommit: ((Double) -> Void)?
    /// Fires when a touch lands on the slider — the "interaction begins"
    /// hook the reader uses to capture its reflow anchor.
    var onTouchDown: (() -> Void)?

    private(set) var value: Double

    private let config: Config
    private let slider: StepSliderControl
    private let valueLabel = InkLabel()
    private let feedback = UISelectionFeedbackGenerator()
    private var lastIndex: Int

    init(config: Config, value: Double) {
        self.config = config
        let clamped = min(max(value, config.range.lowerBound), config.range.upperBound)
        self.value = clamped
        lastIndex = Int(((clamped - config.range.lowerBound) / config.step).rounded())
        slider = StepSliderControl()
        super.init(frame: .zero)

        let titleLabel = InkLabel()
        titleLabel.text = config.label
        titleLabel.font = InkFont.ui
        titleLabel.textColor = InkColor.textDisplay

        valueLabel.font = InkFont.caption
        valueLabel.textColor = InkColor.textTertiary
        valueLabel.text = config.format(clamped)

        let header = UIStackView(arrangedSubviews: [titleLabel, UIView(), valueLabel])
        header.axis = .horizontal
        header.alignment = .firstBaseline

        slider.minimumValue = Float(config.range.lowerBound)
        slider.maximumValue = Float(config.range.upperBound)
        slider.value = Float(clamped)
        slider.minimumTrackTintColor = InkColor.accent
        slider.maximumTrackTintColor = InkColor.bgRecessed
        slider.isAccessibilityElement = true
        slider.accessibilityLabel = config.a11yLabel
        slider.accessibilityValue = config.format(clamped)
        slider.onStep = { [weak self] direction in self?.voiceOverStep(direction) }

        slider.addAction(UIAction { [weak self] _ in self?.sliderMoved() }, for: .valueChanged)
        slider.addAction(UIAction { [weak self] _ in self?.onTouchDown?() }, for: .touchDown)
        for event in [UIControl.Event.touchUpInside, .touchUpOutside, .touchCancel] {
            slider.addAction(UIAction { [weak self] _ in self?.released() }, for: event)
        }

        let stack = UIStackView(arrangedSubviews: [header, slider])
        stack.axis = .vertical
        stack.spacing = 4
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

    /// Moves the control to `newValue` without firing preview or commit —
    /// for programmatic resets.
    func setValue(_ newValue: Double) {
        let (snappedValue, index) = snapped(newValue)
        value = snappedValue
        lastIndex = index
        slider.setValue(Float(snappedValue), animated: true)
        renderValue()
    }

    private func snapped(_ raw: Double) -> (value: Double, index: Int) {
        let index = Int(((raw - config.range.lowerBound) / config.step).rounded())
        let value = min(
            max(config.range.lowerBound + Double(index) * config.step, config.range.lowerBound),
            config.range.upperBound
        )
        return (value, index)
    }

    private func sliderMoved() {
        let (snappedValue, index) = snapped(Double(slider.value))
        // Detent feel: the thumb rides the grid, never between steps.
        slider.setValue(Float(snappedValue), animated: false)
        guard index != lastIndex else { return }
        lastIndex = index
        value = snappedValue
        feedback.selectionChanged()
        renderValue()
        onPreview?(snappedValue)
    }

    private func released() {
        onCommit?(value)
    }

    /// VoiceOver adjusts without ever "releasing": every step must both
    /// preview and commit, or the setting would silently never persist.
    private func voiceOverStep(_ direction: Int) {
        let (current, _) = snapped(Double(slider.value))
        let next = min(
            max(current + Double(direction) * config.step, config.range.lowerBound),
            config.range.upperBound
        )
        guard next != value else { return }
        value = next
        lastIndex = snapped(next).index
        slider.setValue(Float(next), animated: false)
        renderValue()
        onPreview?(next)
        onCommit?(next)
    }

    private func renderValue() {
        let text = config.format(value)
        valueLabel.text = text
        slider.accessibilityValue = text
    }
}

/// The underlying slider: routes VoiceOver's increment/decrement to the
/// step grid instead of UISlider's default 10% jumps.
private final class StepSliderControl: UISlider {
    var onStep: ((Int) -> Void)?

    override func accessibilityIncrement() { onStep?(1) }
    override func accessibilityDecrement() { onStep?(-1) }
}
