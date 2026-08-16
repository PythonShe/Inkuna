import os
import ReadiumShared
import UIKit

// The UniFFI bindings are compiled into this target, so `Publication` below
// is the core's record, not `ReadiumShared.Publication` — the import is for
// `Locator`, which decodes the core's stored position.

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

    private let progressBar: InkProgressBar
    private let metaLabel = InkLabel()
    private let contentsStack = UIStackView()

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

        let coverBlock = UIStackView(arrangedSubviews: [cover, titleLabel, authorLabel, progressBar, metaLabel, readButton])
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
                let publication = try await bookshelf.publication(id: id)
                let chapters = try await bookshelf.chapters(id: id)
                guard let self, !Task.isCancelled else { return }
                self.publication = publication
                self.chapters = chapters
                self.progressBar.setProgress(CGFloat(publication.progression), animated: false)
                self.metaLabel.text = self.positionText()
                self.rebuildContents()
            } catch is CancellationError {
                // The screen was popped, or a newer refresh took over.
                return
            } catch {
                logger.warning("Refreshing detail for \(id, privacy: .public) failed: \(error)")
            }
        }
    }

    // MARK: Position line

    /// The honest position line, mirroring the reader: "p. N of M" only
    /// when the stored locator carries a synthetic position and the core
    /// knows the count — book-wide percentage alone otherwise. Never a
    /// fictional page number.
    private func positionText() -> String {
        let percent = Int((publication.progression * 100).rounded())
        if
            let locatorJSON = publication.locator,
            let locator = try? Locator(jsonString: locatorJSON),
            let position = locator.locations.position,
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

    /// The chapter the saved position sits in: the first TOC entry whose
    /// resource matches the stored locator's, the same "several entries in
    /// one resource resolve to the first" rule as the reader's contents
    /// sheet. Unlike the sheet, this screen keeps the book closed, so a
    /// position in a resource that carries no TOC entry of its own cannot
    /// be attributed to the preceding chapter — those books show no
    /// highlight rather than a guessed one.
    private func currentChapterIndex() -> Int? {
        guard
            let locatorJSON = publication.locator,
            let locator = try? Locator(jsonString: locatorJSON)
        else { return nil }
        let resource = ChapterHref.normalized(locator.href.string)
        return chapters.firstIndex { ChapterHref.normalized($0.href) == resource }
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
