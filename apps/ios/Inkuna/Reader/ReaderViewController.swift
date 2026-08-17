import os
import ReadiumNavigator
import ReadiumShared
import ReadiumStreamer
import UIKit

// The UniFFI bindings are compiled into this target, so the core's
// `Publication` record is the module-local `Publication` type and shadows
// `ReadiumShared.Publication`, which is always written fully qualified here.

/// The open reading session's id, held outside the view controller.
///
/// Session transitions ride the serialized core-write chain, which can
/// outlive the reader: a slow predecessor still holds the chain when the
/// reader is popped. The closures own this box outright, so the session that
/// started always gets ended — a session captured `[weak self]` would
/// guard-return on a deallocated reader and leave the row open until the
/// book is next opened, losing every trailing idle minute.
@MainActor
private final class ReaderSession {
    var id: String?
}

/// The reading screen: the core's publication rendered by Readium's
/// `EPUBNavigatorViewController` on the selected page theme, under floating
/// glass chrome — a back button, the position line, and a more button that
/// fans out the reading menu (contents, theme & type, in-book search,
/// bookmark). The chrome shows on entry, tucks away when a page is turned,
/// and comes back on tap.
///
/// The division of labor is the core contract: Readium owns rendering,
/// pagination, and locators; the Rust core owns storage, progress, sessions,
/// and bookmarks. One `updateProgress` per page turn, one session per sitting.
final class ReaderViewController: UIViewController, EPUBNavigatorDelegate {
    /// The core publication being read.
    private let publication: Publication
    /// Where to open instead of the saved position: the chapter the reader
    /// was asked to start at (a detail-screen contents row).
    private let initialChapter: Chapter?

    private var navigator: EPUBNavigatorViewController?
    private var readiumPublication: ReadiumShared.Publication?
    private var navigationAdapter: DirectionalNavigationAdapter?

    /// Reading-order resource lookup: normalized href → reading-order index.
    private var resourceIndexByHref: [String: Int] = [:]
    /// First synthetic position of each reading-order resource, index-aligned.
    private var firstPositionByResource: [Int?] = []
    /// How many synthetic positions each reading-order resource holds,
    /// index-aligned — what turns a hit's in-resource progression into a
    /// page number.
    private var positionCountByResource: [Int] = []
    /// Synthetic position count, reported to the core once known.
    private var positionCount: Int?
    /// The flattened TOC from the core, fetched once at open.
    private var coreChapters: [Chapter] = []

    private var currentLocator: Locator?
    /// Set before restores, jumps, preference reloads, and rotations so the
    /// resulting `locationDidChange` doesn't read as a page turn and tuck the
    /// chrome away.
    private var expectProgrammaticMove = true

    /// The open reading session, captured strongly by the session closures
    /// so it survives this controller.
    private let session = ReaderSession()
    /// Tail of the serialized core-write chain: progress heartbeats and
    /// session transitions must reach the core in the order they happened.
    private var coreWriteChain: Task<Void, Never>?
    /// The open in flight. Owned so popping the reader can cancel it.
    private var openTask: Task<Void, Never>?
    /// The in-flight navigator jump; cancelled when a new jump or teardown wins.
    private var jumpTask: Task<Void, Never>?

    private let logger = Logger(subsystem: "app.inkuna.ios", category: "reader")

    private let loadingIndicator = UIActivityIndicatorView(style: .medium)
    private let openFailureLabel = InkLabel()
    private let dimView = UIView()
    private let pageInfoLabel = InkLabel()
    private let bookmarkFeedback = UIImpactFeedbackGenerator(style: .light)

    // MARK: Floating chrome

    private lazy var backButton = ReaderGlassButton(symbol: "arrow.backward", accessibilityLabel: String(localized: "a11y_back", defaultValue: "Back")) { [weak self] in
        self?.navigationController?.popViewController(animated: true)
    }
    private lazy var menuButton = ReaderGlassButton(symbol: "ellipsis", pointSize: 19, accessibilityLabel: String(localized: "a11y_reading_menu", defaultValue: "Reading menu")) { [weak self] in
        guard let self else { return }
        self.setMenu(visible: !self.menuVisible)
    }
    private lazy var menuView = ReaderMenuView(
        onContents: { [weak self] in
            self?.setMenu(visible: false)
            self?.presentContents()
        },
        onTheme: { [weak self] in
            self?.setMenu(visible: false)
            self?.presentThemeSheet()
        },
        onSearch: { [weak self] in
            self?.setMenu(visible: false)
            self?.showSearch()
        },
        onBookmark: { [weak self] in
            self?.placeBookmark()
        }
    )
    private var menuVisible = false
    private var menuAnimator: UIViewPropertyAnimator?
    private var searchPanel: ReaderSearchPanel?
    /// Chrome shows on entry and on page taps, and gets out of the way
    /// while reading (page turns hide it, taps toggle it).
    private var chromeVisible = true
    private var chromeAnimator: UIViewPropertyAnimator?

    init(publication: Publication, initialChapter: Chapter? = nil) {
        self.publication = publication
        self.initialChapter = initialChapter
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    deinit {
        jumpTask?.cancel()
        NotificationCenter.default.removeObserver(self)
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        let settings = AppSettings.shared
        view.backgroundColor = settings.readingTheme.background

        // MARK: Loading & failure states

        loadingIndicator.color = settings.readingTheme.dimmedForeground
        loadingIndicator.hidesWhenStopped = true
        loadingIndicator.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(loadingIndicator)
        loadingIndicator.startAnimating()

        openFailureLabel.text = String(localized: "reader_open_failed", defaultValue: "This book could not be opened.")
        openFailureLabel.font = InkFont.reading()
        openFailureLabel.textColor = settings.readingTheme.dimmedForeground
        openFailureLabel.textAlignment = .center
        openFailureLabel.numberOfLines = 0
        openFailureLabel.isHidden = true
        openFailureLabel.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(openFailureLabel)

        // MARK: Brightness veil

        dimView.backgroundColor = UIColor(ink: 0x0A0907)
        dimView.isUserInteractionEnabled = false
        dimView.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(dimView)
        applyBrightness(settings.brightness)

        // MARK: Floating chrome

        pageInfoLabel.font = InkFont.caption
        pageInfoLabel.textColor = settings.readingTheme.dimmedForeground
        pageInfoLabel.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(pageInfoLabel)

        menuView.alpha = 0
        menuView.transform = CGAffineTransform(translationX: 0, y: 10)
        menuView.isUserInteractionEnabled = false
        for chrome in [backButton, menuButton, menuView] {
            chrome.translatesAutoresizingMaskIntoConstraints = false
            view.addSubview(chrome)
        }
        NSLayoutConstraint.activate([
            loadingIndicator.centerXAnchor.constraint(equalTo: view.centerXAnchor),
            loadingIndicator.centerYAnchor.constraint(equalTo: view.centerYAnchor),
            openFailureLabel.centerXAnchor.constraint(equalTo: view.centerXAnchor),
            openFailureLabel.centerYAnchor.constraint(equalTo: view.centerYAnchor),
            openFailureLabel.leadingAnchor.constraint(greaterThanOrEqualTo: view.leadingAnchor, constant: InkSpacing.pageMargin),
            dimView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            dimView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            dimView.topAnchor.constraint(equalTo: view.topAnchor),
            dimView.bottomAnchor.constraint(equalTo: view.bottomAnchor),
            backButton.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: InkSpacing.space4),
            backButton.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: 6),
            menuButton.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -InkSpacing.space4),
            menuButton.bottomAnchor.constraint(equalTo: view.safeAreaLayoutGuide.bottomAnchor, constant: -26),
            menuView.trailingAnchor.constraint(equalTo: menuButton.trailingAnchor),
            menuView.bottomAnchor.constraint(equalTo: menuButton.topAnchor, constant: -12),
            // Pills reflow at accessibility sizes instead of overflowing.
            menuView.leadingAnchor.constraint(greaterThanOrEqualTo: view.leadingAnchor, constant: InkSpacing.space4),
            pageInfoLabel.centerXAnchor.constraint(equalTo: view.centerXAnchor),
            pageInfoLabel.bottomAnchor.constraint(equalTo: view.safeAreaLayoutGuide.bottomAnchor),
        ])
        updatePageInfo()

        bookmarkFeedback.prepare()

        // Sessions end when the app leaves the foreground — backgrounded
        // minutes are not reading minutes — and resume when it returns.
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(appDidEnterBackground),
            name: UIApplication.didEnterBackgroundNotification,
            object: nil
        )
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(appWillEnterForeground),
            name: UIApplication.willEnterForegroundNotification,
            object: nil
        )

        openTask = Task { await openPublication() }
    }

    // MARK: Sessions

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        startSession()
        #if DEBUG
        runDebugRouteIfNeeded()
        #endif
    }

    override func viewDidDisappear(_ animated: Bool) {
        super.viewDidDisappear(animated)
        endSession()
        // Leaving for good, not just being covered: nothing is left to open
        // into, and the container's file handles go with us — nothing else
        // ever closes the publication.
        if isMovingFromParent || isBeingDismissed {
            openTask?.cancel()
            jumpTask?.cancel()
            readiumPublication?.close()
            readiumPublication = nil
        }
    }

    @objc private func appDidEnterBackground() {
        endSession()
    }

    @objc private func appWillEnterForeground() {
        if viewIfLoaded?.window != nil {
            startSession()
        }
    }

    private func startSession() {
        enqueueCoreWrite("session start") { [session, id = publication.id] bookshelf in
            guard session.id == nil else { return }
            session.id = try await bookshelf.sessionStart(id: id)
        }
    }

    private func endSession() {
        // The session box is captured strongly on purpose: this close must
        // still run when the chain drains after the reader is gone.
        enqueueCoreWrite("session end") { [session] bookshelf in
            guard let sessionID = session.id else { return }
            session.id = nil
            try await bookshelf.sessionEnd(sessionId: sessionID)
        }
    }

    /// Appends one core write to the serialized chain. Progress updates and
    /// session transitions run strictly in the order they were enqueued, so a
    /// late heartbeat can never land after its session ended. Failures are
    /// logged, never surfaced: losing one heartbeat must not interrupt
    /// reading.
    private func enqueueCoreWrite(
        _ label: String,
        _ work: @escaping @MainActor (Bookshelf) async throws -> Void
    ) {
        let previous = coreWriteChain
        coreWriteChain = Task { @MainActor [logger] in
            await previous?.value
            do {
                let bookshelf = try await LibraryStore.shared.library()
                try await work(bookshelf)
            } catch {
                logger.warning("Core write failed (\(label, privacy: .public)): \(error)")
            }
        }
    }

    #if DEBUG
    // Deep-launch a chrome state for the screenshot loop:
    // `-inkuna.debugScreen reader -inkuna.debugReaderUI menu|search|contents|theme`
    private var didRunDebugRoute = false

    private func runDebugRouteIfNeeded() {
        guard !didRunDebugRoute else { return }
        didRunDebugRoute = true
        switch UserDefaults.standard.string(forKey: "inkuna.debugReaderUI") {
        case "menu": setMenu(visible: true)
        case "search":
            showSearch()
            if let query = UserDefaults.standard.string(forKey: "inkuna.debugSearchQuery") {
                searchPanel?.debugSetQuery(query)
            }
        case "contents": presentContents()
        case "theme": presentThemeSheet()
        case "immersed": setChrome(visible: false)
        default: break
        }
    }
    #endif

    // MARK: Opening

    /// Everything Readium gives back at open, produced off the main actor in
    /// one pass so the publication object can be interrogated freely before
    /// it is handed to the main-actor navigator.
    private struct OpenedBook {
        let publication: ReadiumShared.Publication
        let initialLocation: Locator?
        /// Synthetic positions grouped by reading-order resource.
        let positionsByReadingOrder: [[Locator]]
    }

    private enum ReaderOpenError: Error {
        case fileNotFound
        case assetUnreadable
        case openFailed
    }

    private func openPublication() async {
        let opened: OpenedBook
        do {
            opened = try await Self.openBook(
                path: publication.filePath,
                locatorJSON: publication.locator,
                progression: publication.progression
            )
        } catch {
            logger.error("Opening \(self.publication.id, privacy: .public) failed: \(error)")
            loadingIndicator.stopAnimating()
            openFailureLabel.isHidden = false
            return
        }

        // The reader may have been popped while the book was opening; a
        // navigator installed now would be a child of a hierarchy that is
        // already off screen. The freshly-opened container is closed on
        // every path that does not hand it to `readiumPublication`, whose
        // owner (viewDidDisappear) is the only other closer.
        guard !Task.isCancelled, isViewLoaded else {
            opened.publication.close()
            return
        }

        // Indexed before the navigator exists: the requested start chapter
        // resolves against the reading order, exactly like a contents jump.
        indexResources(of: opened.publication, positionsByReadingOrder: opened.positionsByReadingOrder)
        var initialLocation = opened.initialLocation
        if let initialChapter, let target = jumpLocator(for: initialChapter, in: opened.publication) {
            initialLocation = target
        }

        let navigator: EPUBNavigatorViewController
        do {
            navigator = try EPUBNavigatorViewController(
                publication: opened.publication,
                initialLocation: initialLocation,
                config: EPUBNavigatorViewController.Configuration(preferences: readerPreferences())
            )
        } catch {
            logger.error("Navigator init for \(self.publication.id, privacy: .public) failed: \(error)")
            loadingIndicator.stopAnimating()
            openFailureLabel.isHidden = false
            opened.publication.close()
            return
        }

        readiumPublication = opened.publication
        currentLocator = initialLocation

        navigator.delegate = self
        addChild(navigator)
        navigator.view.frame = view.bounds
        navigator.view.translatesAutoresizingMaskIntoConstraints = false
        view.insertSubview(navigator.view, at: 0)
        NSLayoutConstraint.activate([
            navigator.view.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            navigator.view.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            navigator.view.topAnchor.constraint(equalTo: view.topAnchor),
            navigator.view.bottomAnchor.constraint(equalTo: view.bottomAnchor),
        ])
        navigator.didMove(toParent: self)
        self.navigator = navigator

        // Edge taps turn pages, Apple Books style; center taps fall through
        // to `didTapAt` and toggle the chrome.
        let adapter = DirectionalNavigationAdapter(animatedTransition: true) { [weak self] in
            self?.setChrome(visible: false)
        }
        adapter.bind(to: navigator)
        navigationAdapter = adapter

        loadingIndicator.stopAnimating()
        updatePageInfo()
        reportPositionCountIfNeeded()
        fetchChapters()
    }

    /// Opens the EPUB and interrogates it in one nonisolated pass: asset,
    /// publication, restored location, and synthetic positions.
    private nonisolated static func openBook(
        path: String,
        locatorJSON: String?,
        progression: Double
    ) async throws -> OpenedBook {
        guard let file = FileURL(path: path, isDirectory: false) else {
            throw ReaderOpenError.fileNotFound
        }
        let assetRetriever = AssetRetriever(httpClient: DefaultHTTPClient())
        guard let asset = await assetRetriever.retrieve(url: file).getOrNil() else {
            throw ReaderOpenError.assetUnreadable
        }
        // EPUB only, DRM-free: the core rejects other formats at import and
        // DRM circumvention is out of scope by policy.
        let opener = PublicationOpener(parser: EPUBParser())
        guard let readiumPublication = await opener.open(
            asset: asset,
            allowUserInteraction: false
        ).getOrNil() else {
            throw ReaderOpenError.openFailed
        }

        let positions = await readiumPublication.positionsByReadingOrder().getOrNil() ?? []

        // Restore the saved position: the stored locator verbatim, or — if
        // it is missing or unreadable — the honest fallback of locating the
        // stored book-wide progression.
        var initialLocation = locatorJSON.flatMap { try? Locator(jsonString: $0) }
        if initialLocation == nil, progression > 0 {
            initialLocation = await readiumPublication.locate(progression: progression)
        }

        return OpenedBook(
            publication: readiumPublication,
            initialLocation: initialLocation,
            positionsByReadingOrder: positions
        )
    }

    /// Builds the resource lookups the contents sheet and jump targets use.
    private func indexResources(
        of readiumPublication: ReadiumShared.Publication,
        positionsByReadingOrder: [[Locator]]
    ) {
        var indexByHref: [String: Int] = [:]
        for (index, link) in readiumPublication.readingOrder.enumerated() {
            indexByHref[ChapterHref.normalized(link.href)] = index
        }
        resourceIndexByHref = indexByHref
        firstPositionByResource = positionsByReadingOrder.map { $0.first?.locations.position }
        positionCountByResource = positionsByReadingOrder.map(\.count)
        let count = positionsByReadingOrder.reduce(0) { $0 + $1.count }
        positionCount = count > 0 ? count : nil
    }

    /// Reports the per-resource position ranges to the core on every open.
    /// The ranges carry the position count with them and are what the core
    /// turns into per-chapter spans ("pages left in this chapter").
    private func reportPositionCountIfNeeded() {
        // Always re-report: a matching total can still mean missing
        // per-resource ranges (a library whose books were opened before
        // ranges existed), and the write is one small transaction. An
        // empty report is deliberate — it clears ranges a previous layout
        // left behind so the shells fall back to the percentage caption
        // instead of showing a stale chapter page count.
        let total = positionCount ?? 0
        let counts = total > 0 ? positionCountByResource.map { UInt32(clamping: $0) } : []
        enqueueCoreWrite("report position ranges") { [id = publication.id, logger] bookshelf in
            do {
                try await bookshelf.reportPositionRanges(id: id, counts: counts)
            } catch {
                // The core rejects a breakdown that does not line up with
                // its own spine — it drops duplicate and over-long spine
                // hrefs that Readium's reading order keeps. The per-chapter
                // spans are lost, but "page N of M" need not be.
                logger.warning("Position ranges rejected for \(id, privacy: .public): \(error)")
                guard total > 0 else { return }
                try await bookshelf.reportPositionCount(id: id, count: UInt32(clamping: total))
            }
        }
    }

    private func fetchChapters() {
        Task { [weak self, id = publication.id, logger] in
            do {
                let bookshelf = try await LibraryStore.shared.library()
                let chapters = try await bookshelf.chapters(id: id)
                self?.coreChapters = chapters
            } catch {
                logger.warning("Fetching chapters for \(id, privacy: .public) failed: \(error)")
            }
        }
    }

    // MARK: Progress

    func navigator(_ navigator: Navigator, locationDidChange locator: Locator) {
        currentLocator = locator
        updatePageInfo()

        if expectProgrammaticMove {
            expectProgrammaticMove = false
        } else {
            // Reading takes the page: turning it tucks the chrome away.
            setChrome(visible: false)
        }

        // One `updateProgress` per page turn: opaque locator, book-wide
        // totalProgression, and the synthetic position once known.
        guard
            let locatorJSON = try? locator.jsonString(),
            let totalProgression = locator.locations.totalProgression
        else { return }
        let position = locator.locations.position.map(UInt32.init)
        enqueueCoreWrite("progress") { [id = publication.id] bookshelf in
            try await bookshelf.updateProgress(
                id: id,
                locator: locatorJSON,
                progression: totalProgression,
                position: position
            )
        }
    }

    func navigator(_ navigator: Navigator, presentError error: NavigatorError) {
        logger.warning("Navigator error for \(self.publication.id, privacy: .public): \(error)")
        InkToastView.show(
            symbol: "exclamationmark.triangle",
            text: String(localized: "reader_page_failed", defaultValue: "This page couldn't be shown."),
            in: view,
            topInset: view.safeAreaInsets.top + 56
        )
    }

    func navigator(_ navigator: Navigator, didFailToLoadResourceAt href: RelativeURL, withError error: ReadError) {
        logger.warning("Resource \(href.string, privacy: .public) failed to load: \(error)")
    }

    /**
     Only http(s) leaves the app. An EPUB is untrusted content, and the
     protocol's default implementation opens *any* scheme an installed app
     registers — `tel:`, `shortcuts:`, a bank app's custom scheme; the
     reader is told the link was not followed instead of it firing
     silently. Mirrors the Android shell's external-link guard.
     */
    func navigator(_ navigator: Navigator, presentExternalURL url: URL) {
        let scheme = url.scheme?.lowercased()
        guard scheme == "http" || scheme == "https" else {
            showLinkNotFollowed()
            return
        }
        UIApplication.shared.open(url) { [weak self] opened in
            if !opened { self?.showLinkNotFollowed() }
        }
    }

    private func showLinkNotFollowed() {
        InkToastView.show(
            symbol: "link",
            text: String(localized: "reader_link_failed", defaultValue: "This link could not be opened."),
            in: view,
            topInset: view.safeAreaInsets.top + 56
        )
    }

    // MARK: Chrome

    func navigator(_ navigator: VisualNavigator, didTapAt point: CGPoint) {
        if let searchPanel, searchPanel.alpha > 0 {
            hideSearch()
        } else if menuVisible {
            setMenu(visible: false)
        } else {
            setChrome(visible: !chromeVisible)
        }
    }

    private func setMenu(visible: Bool) {
        guard menuVisible != visible else { return }
        menuVisible = visible
        menuButton.accessibilityLabel = visible
            ? String(localized: "a11y_close_reading_menu", defaultValue: "Close reading menu")
            : String(localized: "a11y_reading_menu", defaultValue: "Reading menu")
        menuView.isUserInteractionEnabled = visible

        menuAnimator?.stopAnimation(true)
        let animator = InkMotion.quietAnimator(duration: 0.24)
        animator.addAnimations {
            self.menuView.alpha = visible ? 1 : 0
            self.menuView.transform = visible ? .identity : CGAffineTransform(translationX: 0, y: 10)
        }
        animator.startAnimation()
        menuAnimator = animator

        if visible {
            UIAccessibility.post(notification: .layoutChanged, argument: menuView)
        }
    }

    private func setChrome(visible: Bool) {
        guard chromeVisible != visible else { return }
        chromeVisible = visible
        if !visible {
            setMenu(visible: false)
        }
        for control in [backButton, menuButton] {
            control.isUserInteractionEnabled = visible
        }
        chromeAnimator?.stopAnimation(true)
        let animator = InkMotion.quietAnimator(duration: 0.24)
        animator.addAnimations {
            for chrome in [self.backButton, self.menuButton, self.pageInfoLabel] {
                chrome.alpha = visible ? 1 : 0
            }
        }
        animator.startAnimation()
        chromeAnimator = animator
    }

    // MARK: Bookmarks

    private func placeBookmark() {
        guard
            let locator = navigator?.currentLocation,
            let locatorJSON = try? locator.jsonString()
        else {
            InkToastView.show(
                symbol: "bookmark.slash",
                text: String(localized: "reader_bookmark_empty", defaultValue: "Nothing to bookmark yet."),
                in: view,
                topInset: view.safeAreaInsets.top + 56
            )
            return
        }
        let progression = locator.locations.totalProgression ?? publication.progression
        bookmarkFeedback.impactOccurred()
        Task { [weak self, id = publication.id, logger] in
            do {
                let bookshelf = try await LibraryStore.shared.library()
                _ = try await bookshelf.addBookmark(id: id, locator: locatorJSON, progression: progression)
                guard let self else { return }
                InkToastView.show(
                    symbol: "bookmark.fill",
                    text: String(localized: "reader_bookmark_placed", defaultValue: "Bookmark placed."),
                    in: self.view,
                    topInset: self.view.safeAreaInsets.top + 56
                )
            } catch {
                logger.warning("Bookmark for \(id, privacy: .public) failed: \(error)")
                guard let self else { return }
                InkToastView.show(
                    symbol: "exclamationmark.triangle",
                    text: String(localized: "reader_bookmark_save_failed", defaultValue: "The bookmark couldn't be saved."),
                    in: self.view,
                    topInset: self.view.safeAreaInsets.top + 56
                )
            }
        }
    }

    // MARK: Contents

    private func presentContents() {
        let currentResourceIndex = currentLocator.flatMap { resourceIndex(forHref: $0.href.string) }
        var rows: [ContentsSheetViewController.Row] = []
        var currentRowIndex: Int?
        // The highlight rule, shared with the Android shell: highlight the
        // last TOC entry whose resource index is <= the current resource
        // index. Several entries inside one resource resolve to the first of
        // them (the reader cannot tell them apart within a resource), and a
        // resource carrying no TOC entry of its own highlights the preceding
        // chapter.
        var bestResourceIndex: Int?
        for (rowIndex, chapter) in coreChapters.enumerated() {
            let chapterResourceIndex = resourceIndex(forHref: chapter.href)
            if
                let chapterResourceIndex,
                let currentResourceIndex,
                chapterResourceIndex <= currentResourceIndex,
                bestResourceIndex.map({ chapterResourceIndex > $0 }) ?? true
            {
                bestResourceIndex = chapterResourceIndex
                currentRowIndex = rowIndex
            }
            rows.append(ContentsSheetViewController.Row(
                chapter: chapter,
                position: chapterResourceIndex.flatMap { firstPositionByResource.indices.contains($0) ? firstPositionByResource[$0] : nil },
                isCurrent: false
            ))
        }
        if let currentRowIndex {
            rows[currentRowIndex].isCurrent = true
        }

        let sheet = ContentsSheetViewController(
            bookTitle: publication.title,
            coverSeed: BookCoverView.coverSeed(for: publication.id),
            coverPath: publication.coverPath,
            rows: rows,
            pageInfoText: pageInfoText()
        )
        sheet.onSelectChapter = { [weak self] chapter in
            self?.jump(to: chapter)
        }
        present(sheet, animated: true)
    }

    private func jump(to chapter: Chapter) {
        guard
            let navigator,
            let readiumPublication,
            let target = jumpLocator(for: chapter, in: readiumPublication)
        else {
            logger.warning("No jump target for chapter \(chapter.id, privacy: .public)")
            return
        }
        expectProgrammaticMove = true
        Task {
            _ = await navigator.go(to: target, options: NavigatorGoOptions(animated: false))
        }
    }

    /// Builds the Readium locator for a core chapter href (resource path,
    /// possibly with a fragment) against the publication's reading order —
    /// the same resolution Readium's own locator service performs, done
    /// synchronously against the indexed reading order.
    private func jumpLocator(for chapter: Chapter, in readiumPublication: ReadiumShared.Publication) -> Locator? {
        guard
            let index = resourceIndex(forHref: chapter.href),
            readiumPublication.readingOrder.indices.contains(index)
        else { return nil }
        let link = readiumPublication.readingOrder[index]
        guard let mediaType = link.mediaType else { return nil }
        let fragment = ChapterHref.splitFragment(chapter.href).fragment
        return Locator(
            href: link.url(),
            mediaType: mediaType,
            title: chapter.title,
            locations: Locator.Locations(
                fragments: fragment.map { [$0] } ?? [],
                progression: fragment == nil ? 0.0 : nil
            )
        )
    }

    private func resourceIndex(forHref href: String) -> Int? {
        resourceIndexByHref[ChapterHref.normalized(href)]
    }

    // MARK: Theme & type

    /// The design system's reading themes and text sizes routed through
    /// Readium's preferences: page colors and font scale are ours, publisher
    /// styles and CJK layout (vertical writing included) stay Readium's.
    private func readerPreferences() -> EPUBPreferences {
        let settings = AppSettings.shared
        let theme = settings.readingTheme
        return EPUBPreferences(
            backgroundColor: ReadiumNavigator.Color(uiColor: theme.background),
            fontSize: settings.textSize.pointSize / ReadingTextSize.medium.pointSize,
            textColor: ReadiumNavigator.Color(uiColor: theme.foreground),
            theme: theme.isNight ? .dark : .light
        )
    }

    private func applyPreferences() {
        guard let navigator else { return }
        // The reload the navigator performs is not a page turn.
        expectProgrammaticMove = true
        navigator.submitPreferences(readerPreferences())
    }

    private func presentThemeSheet() {
        let sheet = ThemeTypeSheetViewController()
        sheet.onThemeChange = { [weak self] theme in
            AppSettings.shared.readingTheme = theme
            guard let self else { return }
            InkMotion.runQuiet {
                self.view.backgroundColor = theme.background
                self.pageInfoLabel.textColor = theme.dimmedForeground
                self.loadingIndicator.color = theme.dimmedForeground
                self.openFailureLabel.textColor = theme.dimmedForeground
            }
            self.applyPreferences()
        }
        sheet.onSizeChange = { [weak self] size in
            AppSettings.shared.textSize = size
            self?.applyPreferences()
        }
        sheet.onBrightnessChange = { [weak self] brightness in
            AppSettings.shared.brightness = brightness
            self?.applyBrightness(brightness)
        }
        present(sheet, animated: true)
    }

    // MARK: In-book search

    private func showSearch() {
        let panel: ReaderSearchPanel
        if let existing = searchPanel {
            panel = existing
        } else {
            panel = ReaderSearchPanel(
                search: { [weak self] query in await self?.runSearch(query) ?? nil },
                positionForHit: { [weak self] hit in self?.position(of: hit) ?? nil }
            )
            panel.onClose = { [weak self] in self?.hideSearch() }
            panel.onJump = { [weak self] hit in
                self?.jump(to: hit)
                self?.hideSearch()
            }
            panel.alpha = 0
            // The panel owns the screen while it is up: VoiceOver must not
            // land on the chrome buried beneath the glass.
            panel.accessibilityViewIsModal = true
            panel.translatesAutoresizingMaskIntoConstraints = false
            view.addSubview(panel)
            NSLayoutConstraint.activate([
                panel.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 14),
                panel.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -14),
                panel.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: 8),
                // Results scroll inside the panel instead of running under
                // the keyboard.
                panel.bottomAnchor.constraint(lessThanOrEqualTo: view.keyboardLayoutGuide.topAnchor, constant: -12),
            ])
            searchPanel = panel
        }
        // The back button sits right beneath the panel's glass — tuck the
        // chrome away so it neither shows through nor stays tappable.
        setChrome(visible: false)
        InkMotion.runQuiet {
            panel.alpha = 1
        }
        panel.focus()
        UIAccessibility.post(notification: .layoutChanged, argument: panel)
    }

    private func hideSearch() {
        guard let panel = searchPanel else { return }
        // Drop the keyboard with the fade; the content reset waits for it.
        panel.endEditing(true)
        let animator = InkMotion.quietAnimator(duration: InkMotion.fast)
        animator.addAnimations {
            panel.alpha = 0
        }
        // Reset after the fade: collapsing the results mid-fade snaps the
        // panel's height.
        animator.addCompletion { _ in
            panel.reset()
        }
        animator.startAnimation()
        setChrome(visible: true)
    }

    /// One in-book search through the core. A failure is not an error the
    /// reader interrupts reading for: the panel shows its empty state and
    /// the reason goes to the log.
    private func runSearch(_ query: String) async -> BookSearchResults? {
        do {
            let bookshelf = try await LibraryStore.shared.library()
            return try await bookshelf.searchInBook(
                id: publication.id,
                query: query,
                limit: ReaderSearchPanel.hitLimit
            )
        } catch {
            logger.warning("Search in \(self.publication.id, privacy: .public) failed: \(error)")
            return nil
        }
    }

    /// The hit's synthetic position: where its resource starts, plus its
    /// in-resource progression across that resource's positions. Nil until
    /// the navigator has computed positions, so the row simply drops the
    /// page line rather than inventing one.
    private func position(of hit: BookSearchHit) -> Int? {
        guard
            let index = resourceIndex(forHref: hit.href),
            firstPositionByResource.indices.contains(index),
            let first = firstPositionByResource[index],
            positionCountByResource.indices.contains(index)
        else { return nil }
        let count = positionCountByResource[index]
        guard count > 0 else { return first }
        let offset = Int((hit.progression * Double(count)).rounded(.down))
        return first + min(max(offset, 0), count - 1)
    }

    /// Jumps to a search hit: its resource, at the hit's in-resource
    /// progression — the same reading-order resolution the contents jumps
    /// use, since the core's href is package-relative like a TOC entry's.
    private func jump(to hit: BookSearchHit) {
        guard
            let navigator,
            let readiumPublication,
            let index = resourceIndex(forHref: hit.href),
            readiumPublication.readingOrder.indices.contains(index)
        else {
            logger.warning("No jump target for hit in \(hit.href, privacy: .public)")
            return
        }
        let link = readiumPublication.readingOrder[index]
        guard let mediaType = link.mediaType else { return }
        let target = Locator(
            href: link.url(),
            mediaType: mediaType,
            locations: Locator.Locations(progression: min(max(hit.progression, 0), 1))
        )
        expectProgrammaticMove = true
        jumpTask?.cancel()
        jumpTask = Task {
            _ = await navigator.go(to: target, options: NavigatorGoOptions(animated: false))
        }
    }

    // MARK: Position

    private func applyBrightness(_ brightness: Double) {
        // The design's veil: alpha ramps up as brightness drops below 78%.
        dimView.alpha = max(0, 0.78 - brightness) / 1.7
    }

    /// The honest position line: synthetic positions once the navigator has
    /// computed them, book-wide progression alone until then. Never a
    /// fictional page number.
    private func pageInfoText() -> String {
        let progression = currentLocator?.locations.totalProgression ?? publication.progression
        let percent = Int((progression * 100).rounded())
        if let position = currentLocator?.locations.position, let positionCount, positionCount > 0 {
            let format = NSLocalizedString("reader_page_info", comment: "")
            return String.localizedStringWithFormat(format, Int64(position), Int64(positionCount), Int64(percent))
        }
        let format = NSLocalizedString("reader_percent", comment: "")
        return String.localizedStringWithFormat(format, Int64(percent))
    }

    private func updatePageInfo() {
        pageInfoLabel.text = pageInfoText()
        let progression = currentLocator?.locations.totalProgression ?? publication.progression
        let percent = Int((progression * 100).rounded())
        let format = NSLocalizedString("reader_menu_contents", comment: "")
        menuView.contentsPill.text = String.localizedStringWithFormat(format, Int64(percent))
    }

    override func viewWillTransition(to size: CGSize, with coordinator: any UIViewControllerTransitionCoordinator) {
        super.viewWillTransition(to: size, with: coordinator)
        // The navigator re-lays out and re-emits its location on rotation;
        // that is not a page turn either.
        expectProgrammaticMove = true
    }
}
