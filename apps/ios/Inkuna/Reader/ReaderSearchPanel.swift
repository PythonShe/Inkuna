import UIKit

/// In-book search: a floating glass panel over the page. Typing runs the
/// core's CJK-aware search over the open book; tapping a result jumps the
/// reader to that hit.
///
/// The panel owns no search logic of its own — it is handed an executor
/// (the reader's call into the core), a position resolver for the "p. N"
/// line, and a jump handler.
final class ReaderSearchPanel: UIView, UITextFieldDelegate {
    /// How many hits the core is asked for; the panel scrolls them and
    /// announces the true `total` when it is larger.
    static let hitLimit: UInt32 = 200
    /// Rows built per query; `total` still reports the book's true count.
    private static let renderLimit = 50
    /// Keystrokes ride out the burst before a search is spent on them.
    private static let debounce = Duration.milliseconds(250)

    var onClose: (@MainActor () -> Void)?
    var onJump: (@MainActor (BookSearchHit) -> Void)?

    /// Runs one query against the core. `nil` means the search could not
    /// be run at all (the panel then shows its empty state).
    private let search: @MainActor (String) async -> BookSearchResults?
    /// The hit's Readium position, for the "p. N" line; `nil` hides it.
    private let positionForHit: @MainActor (BookSearchHit) -> Int?

    private let glass = InkGlassView(cornerRadius: InkRadius.lg)
    private let field = UITextField()
    private let resultsSection = UIStackView()
    private let resultsScroll = UIScrollView()
    private let resultsStack = UIStackView()
    private let emptyLabel = InkLabel()

    /// The debounce-plus-search in flight; every edit cancels it.
    private var searchTask: Task<Void, Never>?

    init(
        search: @escaping @MainActor (String) async -> BookSearchResults?,
        positionForHit: @escaping @MainActor (BookSearchHit) -> Int?
    ) {
        self.search = search
        self.positionForHit = positionForHit
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

        field.attributedPlaceholder = NSAttributedString(
            string: String(localized: "reader_search_placeholder", defaultValue: "Search this book"),
            attributes: [.foregroundColor: InkColor.textTertiary]
        )
        field.font = InkFont.sans(15, weight: .regular, style: .callout)
        field.adjustsFontForContentSizeCategory = true
        field.textColor = InkColor.textDisplay
        field.tintColor = InkColor.accentText
        field.returnKeyType = .search
        field.autocorrectionType = .no
        field.delegate = self
        field.addAction(
            UIAction { [weak self] _ in self?.queryDidChange() },
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
        // Results scroll when the panel is capped by the keyboard.
        resultsStack.translatesAutoresizingMaskIntoConstraints = false
        resultsScroll.addSubview(resultsStack)
        let scrollFit = resultsScroll.heightAnchor.constraint(equalTo: resultsScroll.contentLayoutGuide.heightAnchor)
        scrollFit.priority = .defaultHigh
        NSLayoutConstraint.activate([
            resultsStack.leadingAnchor.constraint(equalTo: resultsScroll.contentLayoutGuide.leadingAnchor),
            resultsStack.trailingAnchor.constraint(equalTo: resultsScroll.contentLayoutGuide.trailingAnchor),
            resultsStack.topAnchor.constraint(equalTo: resultsScroll.contentLayoutGuide.topAnchor),
            resultsStack.bottomAnchor.constraint(equalTo: resultsScroll.contentLayoutGuide.bottomAnchor),
            resultsStack.widthAnchor.constraint(equalTo: resultsScroll.frameLayoutGuide.widthAnchor),
            scrollFit,
        ])

        emptyLabel.text = String(localized: "reader_search_empty", defaultValue: "Nothing found in this book.")
        emptyLabel.font = InkFont.serif(15, weight: .regular, style: .subheadline)
        emptyLabel.textColor = InkColor.textTertiary
        emptyLabel.textAlignment = .center
        emptyLabel.numberOfLines = 0

        resultsSection.axis = .vertical
        resultsSection.spacing = 4
        resultsSection.addArrangedSubview(hairline)
        resultsSection.addArrangedSubview(resultsScroll)
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

    /// Whether the query field owns the keyboard — the reader withholds
    /// its page-turn key commands while it does.
    var isEditing: Bool {
        field.isFirstResponder
    }

    /// Clears the query and drops the keyboard (called when the panel hides).
    func reset() {
        searchTask?.cancel()
        searchTask = nil
        field.text = nil
        field.resignFirstResponder()
        clearResults()
        resultsSection.isHidden = true
    }

    // MARK: Query

    /// A query worth spending a search on. A single Han, Kana or Hangul
    /// character is a whole word — 月 and 书 are real queries — so the
    /// two-character floor is a Latin rule only. Ported from the Android
    /// shell so both shells agree on what "searchable" means.
    static func isSearchable(_ query: String) -> Bool {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.count > 1 || (!trimmed.isEmpty && containsCJK(trimmed))
    }

    private static let cjkScripts = CharacterSet(charactersIn: "\u{3040}"..."\u{30FF}")
        .union(CharacterSet(charactersIn: "\u{3400}"..."\u{4DBF}"))
        .union(CharacterSet(charactersIn: "\u{4E00}"..."\u{9FFF}"))
        .union(CharacterSet(charactersIn: "\u{F900}"..."\u{FAFF}"))
        .union(CharacterSet(charactersIn: "\u{1100}"..."\u{11FF}"))
        .union(CharacterSet(charactersIn: "\u{3130}"..."\u{318F}"))
        .union(CharacterSet(charactersIn: "\u{AC00}"..."\u{D7AF}"))
        .union(CharacterSet(charactersIn: "\u{20000}"..."\u{2FA1F}"))

    private static func containsCJK(_ text: String) -> Bool {
        text.unicodeScalars.contains { cjkScripts.contains($0) }
    }

    /// The core's leading context can run long enough that a two-line
    /// tail-truncating label pushes the match itself off screen. Keep only
    /// the tail of the pre-text — enough to read into the match, never
    /// enough to hide it. Shared with the Search tab's excerpts.
    static func clampedLeadingContext(_ pre: String) -> String {
        let budget = 16
        guard pre.count > budget else { return pre }
        return "…" + pre.suffix(budget)
    }

    #if DEBUG
    /// Screenshot-loop hook: the simulator has no keystroke automation, so
    /// `-inkuna.debugSearchQuery <q>` lands here to type for it.
    func debugSetQuery(_ query: String) {
        field.text = query
        queryDidChange()
    }
    #endif

    /// Each keystroke cancels the search its predecessor was about to run:
    /// a fast typist spends one core search, not one per character.
    private func queryDidChange() {
        searchTask?.cancel()
        let query = (field.text ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        guard Self.isSearchable(query) else {
            searchTask = nil
            clearResults()
            resultsSection.isHidden = true
            return
        }
        searchTask = Task { [weak self] in
            do {
                try await Task.sleep(for: Self.debounce)
            } catch {
                return
            }
            guard let self, !Task.isCancelled else { return }
            let results = await self.search(query)
            guard !Task.isCancelled else { return }
            self.render(results)
        }
    }

    private func clearResults() {
        for row in resultsStack.arrangedSubviews {
            row.removeFromSuperview()
        }
    }

    /// `nil` means the core could not run the search at all — the panel
    /// says so instead of passing failure off as an empty book.
    private func render(_ maybeResults: BookSearchResults?) {
        let results = maybeResults ?? BookSearchResults(hits: [], total: 0)
        clearResults()
        let visibleHits = results.hits.prefix(Self.renderLimit)
        for hit in visibleHits {
            resultsStack.addArrangedSubview(makeResultRow(hit: hit))
        }

        emptyLabel.text = maybeResults == nil
            ? String(localized: "reader_search_unavailable", defaultValue: "Search isn't available right now.")
            : String(localized: "reader_search_empty", defaultValue: "Nothing found in this book.")
        resultsSection.isHidden = false
        resultsScroll.isHidden = results.hits.isEmpty
        emptyLabel.isHidden = !results.hits.isEmpty
        resultsScroll.setContentOffset(.zero, animated: false)

        let announcement: String
        if maybeResults == nil {
            announcement = emptyLabel.text ?? ""
        } else if results.hits.isEmpty {
            announcement = String(localized: "a11y_no_results", defaultValue: "No results")
        } else if results.total > UInt32(visibleHits.count) {
            // Capped: say how many of the book's matches are on screen.
            let format = NSLocalizedString("a11y_result_count_capped", comment: "")
            announcement = String.localizedStringWithFormat(
                format,
                Int64(visibleHits.count),
                Int64(results.total)
            )
        } else {
            let format = NSLocalizedString("a11y_result_count", comment: "")
            announcement = String.localizedStringWithFormat(format, Int64(visibleHits.count))
        }
        UIAccessibility.post(notification: .announcement, argument: announcement)
    }

    private func makeResultRow(hit: BookSearchHit) -> UIView {
        let snippetFont = InkFont.serif(15, weight: .regular, style: .subheadline)
        let snippet = NSMutableAttributedString(
            string: Self.clampedLeadingContext(hit.snippetPre),
            attributes: [.font: snippetFont, .foregroundColor: InkColor.textDisplay]
        )
        // The match keeps the serif face and takes the accent — the row
        // reads as prose with one word inked, not as marked-up text.
        snippet.append(NSAttributedString(
            string: hit.snippetMatch,
            attributes: [
                .font: InkFont.serif(15, weight: .semibold, style: .subheadline),
                .foregroundColor: InkColor.accentText,
            ]
        ))
        snippet.append(NSAttributedString(
            string: hit.snippetPost,
            attributes: [.font: snippetFont, .foregroundColor: InkColor.textDisplay]
        ))

        let snippetLabel = InkLabel()
        snippetLabel.attributedText = snippet
        snippetLabel.numberOfLines = 2
        snippetLabel.lineBreakMode = .byTruncatingTail
        // The scroll-fit constraint sits at the same 750 as a label's
        // default vertical compression resistance; on a long result list
        // the solver resolves that tie by flattening every label to zero
        // height. Rows must win: their height is the truth here.
        snippetLabel.setContentCompressionResistancePriority(.required, for: .vertical)

        let content = UIStackView(arrangedSubviews: [snippetLabel])
        content.axis = .vertical
        content.spacing = 3
        content.isUserInteractionEnabled = false
        content.translatesAutoresizingMaskIntoConstraints = false

        // The page line only shows when there is an honest position for it.
        var pageText: String?
        if let position = positionForHit(hit) {
            let pageFormat = NSLocalizedString("reader_chapter_page", comment: "")
            let text = String.localizedStringWithFormat(pageFormat, Int64(position))
            let whereLabel = InkLabel()
            whereLabel.text = text
            whereLabel.font = InkFont.caption
            whereLabel.textColor = InkColor.textTertiary
            whereLabel.setContentCompressionResistancePriority(.required, for: .vertical)
            content.addArrangedSubview(whereLabel)
            pageText = text
        }

        let row = ResultRowControl { [weak self] in self?.onJump?(hit) }
        row.layer.cornerRadius = InkRadius.sm
        row.addSubview(content)
        NSLayoutConstraint.activate([
            content.leadingAnchor.constraint(equalTo: row.leadingAnchor, constant: 4),
            content.trailingAnchor.constraint(equalTo: row.trailingAnchor, constant: -4),
            content.topAnchor.constraint(equalTo: row.topAnchor, constant: 11),
            content.bottomAnchor.constraint(equalTo: row.bottomAnchor, constant: -11),
        ])

        let spoken = hit.snippetPre + hit.snippetMatch + hit.snippetPost
        row.isAccessibilityElement = true
        row.accessibilityLabel = pageText.map { "\(spoken), \($0)" } ?? spoken
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
