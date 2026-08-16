import UIKit

/// Pill segmented control (design-system `SegmentedControl`): a recessed
/// capsule track whose selected segment floats on a raised chip.
final class InkSegmentedControl: UIControl {
    var onChange: ((String) -> Void)?
    var onSelectIndex: ((Int) -> Void)?

    private(set) var selectedIndex: Int
    var selectedOption: String {
        guard selectedIndex >= 0 && selectedIndex < options.count else { return "" }
        return options[selectedIndex]
    }
    private let options: [String]
    private let stack = UIStackView()
    private let selectionFeedback = UISelectionFeedbackGenerator()

    init(options: [String], selected: String? = nil, selectedIndex: Int? = nil) {
        self.options = options
        if let selectedIndex, selectedIndex >= 0, selectedIndex < options.count {
            self.selectedIndex = selectedIndex
        } else if let selected, let idx = options.firstIndex(of: selected) {
            self.selectedIndex = idx
        } else {
            self.selectedIndex = 0
        }
        super.init(frame: .zero)

        backgroundColor = InkColor.bgRecessed
        selectionFeedback.prepare()
        stack.axis = .horizontal
        stack.spacing = 2
        stack.distribution = .fillEqually
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 3),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -3),
            stack.topAnchor.constraint(equalTo: topAnchor, constant: 3),
            stack.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -3),
        ])

        for (index, option) in options.enumerated() {
            let segment = UIButton(type: .custom)
            var config = UIButton.Configuration.filled()
            config.cornerStyle = .capsule
            config.contentInsets = NSDirectionalEdgeInsets(top: 7, leading: 16, bottom: 7, trailing: 16)
            config.attributedTitle = AttributedString(option, attributes: AttributeContainer([.font: InkFont.label]))
            segment.configuration = config
            segment.configurationUpdateHandler = { [weak self] button in
                guard let self else { return }
                let isSelected = index == self.selectedIndex
                var config = button.configuration
                config?.baseBackgroundColor = isSelected ? InkColor.bgRaised : .clear
                config?.baseForegroundColor = isSelected ? InkColor.textBody : InkColor.textSecondary
                button.configuration = config
            }
            segment.addAction(
                UIAction { [weak self] _ in self?.selectIndex(index) },
                for: .primaryActionTriggered
            )
            stack.addArrangedSubview(segment)
        }
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    override func layoutSubviews() {
        super.layoutSubviews()
        layer.cornerRadius = InkRadius.pill(for: bounds.height)
    }

    func selectIndex(_ index: Int, notify: Bool = true) {
        guard index >= 0, index < options.count, index != selectedIndex else { return }
        selectedIndex = index
        for case let segment as UIButton in stack.arrangedSubviews {
            segment.setNeedsUpdateConfiguration()
        }
        if notify {
            selectionFeedback.selectionChanged()
            selectionFeedback.prepare()
            onSelectIndex?(index)
            onChange?(options[index])
            sendActions(for: .valueChanged)
        }
    }

    func select(_ option: String, notify: Bool = true) {
        guard let idx = options.firstIndex(of: option) else { return }
        selectIndex(idx, notify: notify)
    }
}
