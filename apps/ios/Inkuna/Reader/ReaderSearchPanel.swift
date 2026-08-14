import UIKit

/// In-book search: a floating glass panel over the page. Typing filters the
/// open book live; tapping a result jumps the reader to that page.
///
/// TODO(core): search moves to the Rust core (CJK-aware segmentation is a
/// product goal); the placeholder scans the sample pages so the interaction
/// is real end to end.
final class ReaderSearchPanel: UIView, UITextFieldDelegate {
    var onClose: (@MainActor () -> Void)?
    var onJump: (@MainActor (Int) -> Void)?

    private let glass = InkGlassView(cornerRadius: InkRadius.lg)
    private let field = UITextField()
    private let resultsSection = UIStackView()
    private let resultsStack = UIStackView()
    private let emptyLabel = InkLabel()
    /// Page number of the first sample page ("p. 564").
    private let basePage: Int

    init(basePage: Int) {
        self.basePage = basePage
        super.init(frame: .zero)

        glass.translatesAutoresizingMaskIntoConstraints = false
        addSubview(glass)

        let glyph = UIImageView(
            image: UIImage(
                systemName: "magnifyingglass",
                withConfiguration: UIImage.SymbolConfiguration(pointSize: 17, weight: .regular)
            )
        )
        glyph.tintColor = InkColor.textTertiary
        glyph.setContentHuggingPriority(.required, for: .horizontal)

        // TODO(l10n): localize once the strings pass lands.
        field.attributedPlaceholder = NSAttributedString(
            string: "Search this book",
            attributes: [.foregroundColor: InkColor.textTertiary]
        )
        field.font = InkFont.sans(15, weight: .regular, style: .callout)
        field.adjustsFontForContentSizeCategory = true
        field.textColor = InkColor.textDisplay
        field.tintColor = InkColor.accent
        field.returnKeyType = .search
        field.autocorrectionType = .no
        field.delegate = self
        field.addAction(
            UIAction { [weak self] _ in self?.updateResults() },
            for: .editingChanged
        )

        let closeButton = InkCloseButton { [weak self] in self?.onClose?() }

        let topRow = UIStackView(arrangedSubviews: [glyph, field, closeButton])
        topRow.axis = .horizontal
        topRow.alignment = .center
        topRow.spacing = 10

        let hairline = UIView()
        hairline.backgroundColor = InkColor.borderHairline
        NSLayoutConstraint.activate([
            hairline.heightAnchor.constraint(equalToConstant: 1 / traitCollection.displayScale),
        ])

        resultsStack.axis = .vertical

        emptyLabel.text = "Nothing found in this book."
        emptyLabel.font = InkFont.serif(15, weight: .regular, style: .subheadline)
        emptyLabel.textColor = InkColor.textTertiary
        emptyLabel.textAlignment = .center
        emptyLabel.numberOfLines = 0

        resultsSection.axis = .vertical
        resultsSection.spacing = 4
        resultsSection.addArrangedSubview(hairline)
        resultsSection.addArrangedSubview(resultsStack)
        resultsSection.addArrangedSubview(emptyLabel)
        resultsSection.setCustomSpacing(10, after: hairline)
        resultsSection.isHidden = true

        let column = UIStackView(arrangedSubviews: [topRow, resultsSection])
        column.axis = .vertical
        column.spacing = 10
        column.translatesAutoresizingMaskIntoConstraints = false
        addSubview(column)

        NSLayoutConstraint.activate([
            glass.leadingAnchor.constraint(equalTo: leadingAnchor),
            glass.trailingAnchor.constraint(equalTo: trailingAnchor),
            glass.topAnchor.constraint(equalTo: topAnchor),
            glass.bottomAnchor.constraint(equalTo: bottomAnchor),
            column.leadingAnchor.constraint(equalTo: leadingAnchor, constant: InkSpacing.space4),
            column.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -InkSpacing.space4),
            column.topAnchor.constraint(equalTo: topAnchor, constant: 8),
            column.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -InkSpacing.space3),
        ])
        NSLayoutConstraint.activate([
            field.heightAnchor.constraint(greaterThanOrEqualToConstant: 40),
        ])
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    func focus() {
        field.becomeFirstResponder()
    }

    /// Clears the query and drops the keyboard (called when the panel hides).
    func reset() {
        field.text = nil
        field.resignFirstResponder()
        updateResults()
    }

    // MARK: Placeholder search

    private func updateResults() {
        for row in resultsStack.arrangedSubviews {
            row.removeFromSuperview()
        }

        let query = (field.text ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        // Match the design: search from two characters.
        // TODO(core): the core's CJK-aware search should match single Han
        // characters; this two-character floor is Latin-placeholder only.
        guard query.count > 1 else {
            resultsSection.isHidden = true
            return
        }

        var found = 0
        for (index, paragraphs) in PlaceholderLibrary.samplePages.enumerated() {
            let text = paragraphs.joined(separator: " ")
            guard let range = text.range(of: query, options: [.caseInsensitive, .diacriticInsensitive]) else { continue }
            found += 1
            let start = text.index(range.lowerBound, offsetBy: -34, limitedBy: text.startIndex) ?? text.startIndex
            let end = text.index(range.upperBound, offsetBy: 46, limitedBy: text.endIndex) ?? text.endIndex
            var snippet = String(text[start..<end]).trimmingCharacters(in: .whitespaces)
            if start > text.startIndex { snippet = "…" + snippet }
            if end < text.endIndex { snippet += "…" }
            resultsStack.addArrangedSubview(
                makeResultRow(snippet: snippet, pageNumber: basePage + index, pageIndex: index)
            )
        }

        resultsSection.isHidden = false
        resultsStack.isHidden = found == 0
        emptyLabel.isHidden = found > 0
    }

    private func makeResultRow(snippet: String, pageNumber: Int, pageIndex: Int) -> UIView {
        let snippetLabel = InkLabel()
        snippetLabel.text = snippet
        snippetLabel.font = InkFont.serif(15, weight: .regular, style: .subheadline)
        snippetLabel.textColor = InkColor.textDisplay
        snippetLabel.numberOfLines = 2

        let whereLabel = InkLabel()
        whereLabel.text = "p. \(pageNumber)"
        whereLabel.font = InkFont.caption
        whereLabel.textColor = InkColor.textTertiary

        let row = ResultRowControl { [weak self] in self?.onJump?(pageIndex) }
        row.layer.cornerRadius = InkRadius.sm

        let content = UIStackView(arrangedSubviews: [snippetLabel, whereLabel])
        content.axis = .vertical
        content.spacing = 3
        content.isUserInteractionEnabled = false
        content.translatesAutoresizingMaskIntoConstraints = false
        row.addSubview(content)
        NSLayoutConstraint.activate([
            content.leadingAnchor.constraint(equalTo: row.leadingAnchor, constant: 4),
            content.trailingAnchor.constraint(equalTo: row.trailingAnchor, constant: -4),
            content.topAnchor.constraint(equalTo: row.topAnchor, constant: 11),
            content.bottomAnchor.constraint(equalTo: row.bottomAnchor, constant: -11),
        ])

        row.isAccessibilityElement = true
        row.accessibilityLabel = "\(snippet), page \(pageNumber)"
        row.accessibilityTraits = .button
        return row
    }

    /// Result row with a soft pressed wash.
    private final class ResultRowControl: UIControl {
        init(handler: @escaping @MainActor () -> Void) {
            super.init(frame: .zero)
            addAction(UIAction { _ in handler() }, for: .touchUpInside)
        }

        @available(*, unavailable)
        required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

        override var isHighlighted: Bool {
            didSet { backgroundColor = isHighlighted ? InkColor.accentSoft : .clear }
        }
    }

    // MARK: UITextFieldDelegate

    func textFieldShouldReturn(_ textField: UITextField) -> Bool {
        textField.resignFirstResponder()
        return true
    }
}
