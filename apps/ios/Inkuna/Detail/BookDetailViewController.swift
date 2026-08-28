import os
import UIKit

// The UniFFI bindings are compiled into this target. Saved progress is a core
// content coordinate projected into synthetic positions by `ReaderPositions`.

/// Book detail: the cover held at arm's length, progress, and the core's
/// table of contents with the saved position's chapter inked in accent.
final class BookDetailViewController: UIViewController {
    /// The book being described. Re-fetched on every appearance: progress
    /// moves while the reader is open, and the rows behind it are stale by
    /// the time the reader is popped.
    private var publication: Publication

    /// The core's flattened TOC; empty until fetched (and for books that
    /// list none).
    private var chapters: [Chapter] = []

    /// The synthetic position the saved coordinate lands on, as the core
    /// derives it. Nil for a book with no stored coordinate — never
    /// opened, or a legacy row the rebaseline has not converted — and the
    /// screen then shows the percentage alone rather than a made-up page.
    private var storedPosition: UInt32?

    /// The core's chapter spans, used only to attribute `storedPosition`
    /// to a TOC entry for the highlight.
    private var chapterRanges: [ChapterPositionRange] = []

    private let progressBar: InkProgressBar
    private let metaLabel = InkLabel()
    private let contentsStack = UIStackView()

    /// Explicit shelf toggle: "Mark as Finished" until the book is
    /// finished, "Move back to Reading" once it is. The title tracks the
    /// re-fetched publication on every refresh.
    private lazy var finishedButton = InkButton("", variant: .secondary) { [weak self] in
        self?.toggleFinished()
    }

    private let logger = Logger(subsystem: "app.inkuna.ios", category: "detail")

    /// The in-flight refresh, cancelled by its successor.
    /// `nonisolated(unsafe)` so the nonisolated `deinit` can cancel it; only
    /// ever touched on the main actor.
    nonisolated(unsafe) private var refreshTask: Task<Void, Never>?

    deinit {
        refreshTask?.cancel()
    }

    init(publication: Publication) {
        self.publication = publication
        self.progressBar = InkProgressBar(progress: CGFloat(publication.progression))
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = InkColor.bgApp

        let scrollView = UIScrollView()
        scrollView.alwaysBounceVertical = true
        scrollView.showsVerticalScrollIndicator = false
        scrollView.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(scrollView)

        let content = UIStackView()
        content.axis = .vertical
        content.translatesAutoresizingMaskIntoConstraints = false
        scrollView.addSubview(content)

        NSLayoutConstraint.activate([
            scrollView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            scrollView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            scrollView.topAnchor.constraint(equalTo: view.topAnchor),
            scrollView.bottomAnchor.constraint(equalTo: view.bottomAnchor),
            content.leadingAnchor.constraint(equalTo: scrollView.contentLayoutGuide.leadingAnchor, constant: InkSpacing.pageMargin),
            content.trailingAnchor.constraint(equalTo: scrollView.contentLayoutGuide.trailingAnchor, constant: -InkSpacing.pageMargin),
            content.topAnchor.constraint(equalTo: scrollView.contentLayoutGuide.topAnchor, constant: InkSpacing.space3),
            content.bottomAnchor.constraint(equalTo: scrollView.contentLayoutGuide.bottomAnchor, constant: -InkSpacing.space16),
            content.widthAnchor.constraint(equalTo: scrollView.frameLayoutGuide.widthAnchor, constant: -2 * InkSpacing.pageMargin),
        ])
        scrollView.contentInsetAdjustmentBehavior = .automatic

        // MARK: Back

        let backButton = InkIconButton(symbol: "chevron.backward", accessibilityLabel: String(localized: "a11y_back", defaultValue: "Back")) { [weak self] in
            self?.navigationController?.popViewController(animated: true)
        }
        let backRow = UIStackView(arrangedSubviews: [backButton, UIView()])
        backRow.axis = .horizontal
        content.addArrangedSubview(backRow)
        content.setCustomSpacing(18, after: backRow)

        // MARK: Cover block

        let author = publication.displayAuthors(unknownAuthor: String(localized: "unknown_author", defaultValue: "Unknown author"))
        let cover = BookCoverView(
            title: publication.title,
            author: author,
            seed: BookCoverView.coverSeed(for: publication.id),
            coverPath: publication.coverPath
        )
        cover.widthAnchor.constraint(equalToConstant: 150).isActive = true

        let titleLabel = InkLabel()
        titleLabel.text = publication.title
        titleLabel.font = InkFont.displaySmall
        titleLabel.textColor = InkColor.textDisplay
        titleLabel.textAlignment = .center
        titleLabel.numberOfLines = 0

        let authorLabel = InkLabel()
        authorLabel.text = author
        authorLabel.font = InkFont.labelRegular
        authorLabel.textColor = InkColor.textSecondary

        progressBar.widthAnchor.constraint(equalToConstant: 200).isActive = true

        metaLabel.text = positionText()
        metaLabel.font = InkFont.caption
        metaLabel.textColor = InkColor.textTertiary

        let readButton = InkButton(String(localized: "tonight_keep_reading", defaultValue: "Keep reading"), symbol: "book") { [weak self] in
            guard let self else { return }
            ReaderLauncher.push(self.publication, on: self.navigationController)
        }

        updateFinishedButton()

        let coverBlock = UIStackView(arrangedSubviews: [cover, titleLabel, authorLabel, progressBar, metaLabel, readButton, finishedButton])
        coverBlock.axis = .vertical
        coverBlock.alignment = .center
        coverBlock.spacing = InkSpacing.space2
        coverBlock.setCustomSpacing(22, after: cover)
        coverBlock.setCustomSpacing(InkSpacing.space5, after: authorLabel)
        coverBlock.setCustomSpacing(18, after: metaLabel)
        content.addArrangedSubview(coverBlock)
        content.setCustomSpacing(InkSpacing.space10, after: coverBlock)

        // MARK: Contents

        let contentsTitle = InkLabel()
        contentsTitle.text = String(localized: "detail_contents", defaultValue: "Contents")
        contentsTitle.font = InkFont.sectionTitle
        contentsTitle.textColor = InkColor.textDisplay
        content.addArrangedSubview(contentsTitle)
        content.setCustomSpacing(InkSpacing.space2, after: contentsTitle)

        contentsStack.axis = .vertical
        content.addArrangedSubview(contentsStack)
    }

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)
        refresh()
    }

    /// Re-fetches the publication and its TOC, then repaints progress and
    /// the chapter list. A failed fetch keeps what is already on screen —
    /// the screen was handed a real publication and can stand on it.
    private func refresh() {
        // One refresh at a time: a stale fetch must not repaint over a
        // newer one when appearances come in quick succession.
        refreshTask?.cancel()
        refreshTask = Task { [weak self, id = publication.id, logger] in
            do {
                let bookshelf = try await LibraryStore.shared.library()
                let publication = try await bookshelf.library().publication(id: id)
                let chapters = try await bookshelf.library().chapters(id: id)
                // The position line and the chapter highlight both hang on
                // the stored coordinate; without one there is nothing to
                // ask the core about, and both degrade rather than guess.
                var position: UInt32?
                var ranges: [ChapterPositionRange] = []
                if let coordinate = publication.coordinate {
                    position = try? await ReaderPositions.position(
                        of: coordinate,
                        id: id,
                        on: bookshelf
                    )
                    ranges = (try? await bookshelf.progress().chapterPositionRanges(id: id)) ?? []
                }
                guard let self, !Task.isCancelled else { return }
                self.publication = publication
                self.chapters = chapters
                self.storedPosition = position
                self.chapterRanges = ranges
                self.progressBar.setProgress(CGFloat(publication.progression), animated: false)
                self.metaLabel.text = self.positionText()
                self.updateFinishedButton()
                self.rebuildContents()
            } catch is CancellationError {
                // The screen was popped, or a newer refresh took over.
                return
            } catch {
                logger.warning("Refreshing detail for \(id, privacy: .public) failed: \(error)")
            }
        }
    }

    // MARK: Finished toggle

    /// Repaints the toggle's title and symbol off the current publication.
    /// Configuration surgery rather than a rebuilt button: the instance
    /// stays in the stack and keeps its action and press animation.
    private func updateFinishedButton() {
        let finished = publication.finishedAt != nil
        let title = finished
            ? String(localized: "detail_move_to_reading", defaultValue: "Move back to Reading")
            : String(localized: "detail_mark_finished", defaultValue: "Mark as Finished")
        let symbol = finished ? "arrow.uturn.backward" : "checkmark.circle"
        var config = finishedButton.configuration
        config?.attributedTitle = AttributedString(title, attributes: AttributeContainer([.font: InkFont.ui]))
        config?.image = UIImage(
            systemName: symbol,
            withConfiguration: UIImage.SymbolConfiguration(pointSize: InkFont.ui.pointSize, weight: .medium)
        )
        config?.imagePadding = InkSpacing.space2
        finishedButton.configuration = config
    }

    /// Writes the flipped finished state through the core, then re-fetches
    /// so the title, shelf membership, and progress all come back from the
    /// same source of truth. Un-finishing sticks even at end-of-book:
    /// auto-finish only fires on an upward crossing of the threshold.
    private func toggleFinished() {
        let target = publication.finishedAt == nil
        finishedButton.isEnabled = false
        Task { [weak self, id = publication.id, logger] in
            do {
                let bookshelf = try await LibraryStore.shared.library()
                try await bookshelf.progress().setFinished(id: id, finished: target)
            } catch {
                logger.warning("Toggling finished for \(id, privacy: .public) failed: \(error)")
            }
            guard let self else { return }
            self.finishedButton.isEnabled = true
            self.refresh()
        }
    }

    // MARK: Position line

    /// The honest position line, entirely in the core's position space:
    /// "p. N of M" only when the book carries a coordinate the core can
    /// resolve to a position, and knows its count — book-wide percentage
    /// alone otherwise. Never a fictional page number.
    ///
    /// The reader and this screen use the same core-derived position space,
    /// so the saved coordinate, page-info line, and chapter highlight agree.
    private func positionText() -> String {
        let percent = Int((publication.progression * 100).rounded())
        if
            let position = storedPosition,
            let positionCount = publication.positionCount, positionCount > 0
        {
            let format = NSLocalizedString("reader_page_info", comment: "")
            return String.localizedStringWithFormat(format, Int64(position), Int64(positionCount), Int64(percent))
        }
        let format = NSLocalizedString("reader_percent", comment: "")
        return String.localizedStringWithFormat(format, Int64(percent))
    }

    // MARK: Contents

    private func rebuildContents() {
        contentsStack.arrangedSubviews.forEach { $0.removeFromSuperview() }

        guard !chapters.isEmpty else {
            let empty = InkLabel()
            empty.text = String(localized: "detail_no_contents", defaultValue: "This book lists no contents.")
            empty.font = InkFont.reading()
            empty.textColor = InkColor.textTertiary
            empty.numberOfLines = 0
            let wrapper = UIStackView(arrangedSubviews: [empty])
            wrapper.axis = .vertical
            wrapper.isLayoutMarginsRelativeArrangement = true
            wrapper.layoutMargins = UIEdgeInsets(top: InkSpacing.space4, left: 0, bottom: InkSpacing.space4, right: 0)
            contentsStack.addArrangedSubview(wrapper)
            return
        }

        let currentIndex = currentChapterIndex()
        for (index, chapter) in chapters.enumerated() {
            contentsStack.addArrangedSubview(chapterRow(chapter, isCurrent: index == currentIndex))
        }
    }

    /// The chapter the saved position sits in, attributed by the core's
    /// own chapter spans rather than by matching hrefs here. Those spans
    /// are sparse — one per TOC chapter, never one per spine resource — so
    /// a position inside a resource carrying no TOC entry of its own is
    /// claimed by no span and leaves the list unhighlighted rather than
    /// guessed. The reader's contents sheet is looser and keeps the
    /// preceding chapter lit there; this screen deliberately does not.
    private func currentChapterIndex() -> Int? {
        guard
            let position = storedPosition,
            let range = ReaderPositions.chapterRange(in: chapterRanges, at: position)
        else { return nil }
        return chapters.firstIndex { $0.idx == range.chapterIdx }
    }

    private func chapterRow(_ chapter: Chapter, isCurrent: Bool) -> UIView {
        let numeral = InkLabel()
        numeral.text = "\(chapter.idx + 1)"
        numeral.font = InkFont.caption
        numeral.textColor = isCurrent ? InkColor.accentText : InkColor.textTertiary
        numeral.widthAnchor.constraint(greaterThanOrEqualToConstant: 26).isActive = true
        numeral.setContentHuggingPriority(.required, for: .horizontal)
        numeral.setContentCompressionResistancePriority(.required, for: .horizontal)

        let title = InkLabel()
        title.text = chapter.title
        title.font = InkFont.serif(16, weight: isCurrent ? .semibold : .regular, style: .body)
        title.textColor = isCurrent ? InkColor.accentText : InkColor.textDisplay
        title.numberOfLines = 0

        let row = UIStackView(arrangedSubviews: [numeral, title, UIView()])
        row.axis = .horizontal
        row.alignment = .firstBaseline
        row.spacing = InkSpacing.space3
        row.isLayoutMarginsRelativeArrangement = true
        // Nested TOC entries step in with their depth.
        let leadingInset = 2 + CGFloat(min(chapter.depth, 4)) * 14
        row.layoutMargins = UIEdgeInsets(top: 13, left: leadingInset, bottom: 13, right: 2)
        row.isUserInteractionEnabled = false

        let container = ChapterRowControl { [weak self] in
            guard let self else { return }
            ReaderLauncher.push(self.publication, startingAt: chapter, on: self.navigationController)
        }
        row.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(row)

        let separator = UIView()
        separator.backgroundColor = InkColor.borderHairline
        separator.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(separator)

        NSLayoutConstraint.activate([
            row.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            row.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            row.topAnchor.constraint(equalTo: container.topAnchor),
            row.bottomAnchor.constraint(equalTo: container.bottomAnchor),
            separator.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            separator.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            separator.bottomAnchor.constraint(equalTo: container.bottomAnchor),
            separator.heightAnchor.constraint(equalToConstant: 1 / traitCollection.displayScale),
        ])

        container.isAccessibilityElement = true
        let chapterRowFormat = NSLocalizedString("a11y_chapter_row_no_page", comment: "")
        container.accessibilityLabel = String.localizedStringWithFormat(chapterRowFormat, "\(chapter.idx + 1)", chapter.title)
        container.accessibilityTraits = isCurrent ? [.button, .selected] : .button
        return container
    }

    /// Tappable chapter row with the quiet pressed dim the shelf uses.
    private final class ChapterRowControl: UIControl {
        init(handler: @escaping @MainActor () -> Void) {
            super.init(frame: .zero)
            addAction(UIAction { _ in handler() }, for: .touchUpInside)
        }

        @available(*, unavailable)
        required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

        override var isHighlighted: Bool {
            didSet {
                guard isHighlighted != oldValue else { return }
                let pressed = isHighlighted
                InkMotion.runQuiet(duration: InkMotion.fast) {
                    self.alpha = pressed ? 0.7 : 1
                }
            }
        }
    }
}
