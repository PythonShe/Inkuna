import os
import ReadiumNavigator
import ReadiumShared
import ReadiumStreamer
import UIKit
import WebKit

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
final class ReaderViewController: UIViewController, EPUBNavigatorDelegate, ReaderAccessibilityScrolling {
    /// The core publication being read.
    private let publication: Publication
    /// Where to open instead of the saved position: the chapter the reader
    /// was asked to start at (a detail-screen contents row).
    private let initialChapter: Chapter?

    private var navigator: EPUBNavigatorViewController?

    /// The Readium-shaped styling surface behind `pager` — the walk that
    /// finds the loaded spread web views for user-stylesheet pushes.
    private var pagerSurface: ReadiumPagerSurface?

    /// The live user stylesheet, read by the container transform whenever
    /// a resource is served. Seeded before the book opens.
    private let userStyleBox = ReaderUserStyleBox()

    /// True while the Customize panel is previewing an uncommitted style —
    /// the reflow re-landing fires a locationDidChange per step, and those
    /// must not each queue a core progress write.
    private var liveStyleSession = false
    private var readiumPublication: ReadiumShared.Publication?
    private var pager: ReaderPager?

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
    /// Armed while a VoiceOver-requested turn is on its way to settling:
    /// the position announcement waits for the landing, and this is its
    /// backstop if no location ever arrives.
    private var pendingScrollAnnouncement: Task<Void, Never>?
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
        pendingScrollAnnouncement?.cancel()
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
            pageInfoLabel.bottomAnchor.constraint(
                equalTo: view.safeAreaLayoutGuide.bottomAnchor,
                constant: -ReaderMetrics.footerLift
            ),
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

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)
        // Belt and braces beside `viewDidAppear`: any route back to the
        // reader — a popped push, a dismissed sheet whose delegate never
        // fired — finds the chain retaken before the first frame.
        takeKeyCommandChain()
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        takeKeyCommandChain()
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
            session.id = try await bookshelf.stats().sessionStart(id: id)
        }
    }

    private func endSession() {
        // The session box is captured strongly on purpose: this close must
        // still run when the chain drains after the reader is gone.
        enqueueCoreWrite("session end") { [session] bookshelf in
            guard let sessionID = session.id else { return }
            session.id = nil
            try await bookshelf.stats().sessionEnd(sessionId: sessionID)
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
        case "customize", "fontmenu":
            presentThemeSheet()
            let openMenu = UserDefaults.standard.string(forKey: "inkuna.debugReaderUI") == "fontmenu"
            let panel = makeCustomizePanel()
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.6) { [weak self] in
                guard let host = self?.presentedViewController as? UINavigationController else { return }
                host.pushViewController(panel, animated: false)
                guard openMenu else { return }
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.4) {
                    panel.debugOpenFontMenu()
                }
            }
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
            userStyleBox.write(ReaderUserStyle.current.css())
            opened = try await Self.openBook(
                path: publication.filePath,
                locatorJSON: publication.locator,
                progression: publication.progression,
                style: userStyleBox
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
            var config = EPUBNavigatorViewController.Configuration(preferences: readerPreferences())
            // Serves the bundled Noto files to every spread; which family
            // the page uses is our stylesheet's decision, never a Readium
            // preference. See ReadingFontDeclarations.
            config.fontFamilyDeclarations = ReadingFont.fontFamilyDeclarations
            navigator = try EPUBNavigatorViewController(
                publication: opened.publication,
                initialLocation: initialLocation,
                config: config
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

        // The pager owns every horizontal page turn — inner pages and
        // chapter boundaries alike — on our gestures and physics;
        // Readium's native paging is suppressed. See ReaderPager.
        let surface = ReadiumPagerSurface(navigator: navigator)
        pagerSurface = surface
        let pager = ReaderPager(
            surface: surface,
            view: navigator.view
        )
        // Chrome leaves at gesture claim, not on arrival: hiding via
        // locationDidChange keeps a live backdrop blur composited over
        // the entire turn (Readium reports the location only after the
        // settle plus its own debounce). locationDidChange stays as the
        // fallback for programmatic moves.
        // Under VoiceOver the chrome stays: a page turn there is a scroll
        // action, and the buttons it would strip are how the reader
        // navigates. The Android shell suppresses the same hide under
        // touch exploration.
        pager.onPageTurnGesture = { [weak self] in
            guard !UIAccessibility.isVoiceOverRunning else { return }
            self?.setChrome(visible: false)
        }
        self.pager = pager

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
        progression: Double,
        style: ReaderUserStyleBox
    ) async throws -> OpenedBook {
        guard let file = FileURL(path: path, isDirectory: false) else {
            throw ReaderOpenError.fileNotFound
        }
        let assetRetriever = AssetRetriever(httpClient: DefaultHTTPClient())
        guard let asset = await assetRetriever.retrieve(url: file).getOrNil() else {
            throw ReaderOpenError.assetUnreadable
        }
        // EPUB only, DRM-free: the core rejects other formats at import and
        // DRM circumvention is out of scope by policy. Every XHTML resource
        // gets the fragmentation fix and the user stylesheet injected
        // before WebKit ever paginates it; see injectingInkunaStyles.
        let opener = PublicationOpener(parser: EPUBParser()) { _, container, _ in
            container = injectingInkunaStyles(container, style: style)
        }
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

    /// Books frequently ship `page-break-inside: avoid` on whole paragraphs
    /// or wrapper divs; the column fragmenter then carries the entire block
    /// to the next page, leaving the bottom of the previous one blank.
    /// Appended after the author's styles, this lets running text fragment
    /// normally again. Headings, figures, images and tables keep Readium
    /// CSS's own keep-together rules — small and typographically right.
    private nonisolated static let fragmentationFixStyle =
        "<style>" +
        "p, blockquote, li, dd, div, section, aside {" +
        "break-inside: auto !important;" +
        "page-break-inside: auto !important;" +
        "-webkit-column-break-inside: auto !important;" +
        "}" +
        "</style>"

    /// Injects `fragmentationFixStyle` plus the live user stylesheet at
    /// the end of each XHTML resource's `<head>`. The splice is done on
    /// raw bytes, never through a decoded string: the ASCII markers
    /// survive any ASCII-compatible encoding (including legacy CJK ones)
    /// unchanged, and in UTF-16 the marker simply isn't found — the
    /// resource passes through untouched instead of being blanked by a
    /// failed decode. NCX and non-XHTML resources are never touched.
    ///
    /// The user CSS is read from `style` at *serve* time, not open time:
    /// a spread preloaded after a settings change must paint with the new
    /// typography rather than flash and reflow.
    private nonisolated static func injectingInkunaStyles(
        _ container: Container,
        style: ReaderUserStyleBox
    ) -> Container {
        // XHTML mandates lowercase; the uppercase form covers stray HTML.
        let markers = [Data("</head>".utf8), Data("</HEAD>".utf8)]
        return container.map { href, resource in
            guard
                let ext = href.pathExtension?.rawValue,
                ["xhtml", "html", "htm"].contains(ext)
            else { return resource }
            return TransformingResource(resource) { result in
                result.map { data in
                    guard let head = markers.lazy
                        .compactMap({ data.range(of: $0, options: .backwards) })
                        .first
                    else { return data }
                    let splice = fragmentationFixStyle
                        + "<style id=\"\(ReaderUserStyle.styleElementID)\">\(style.read())</style>"
                    var fixed = data
                    fixed.insert(contentsOf: Data(splice.utf8), at: head.lowerBound)
                    return fixed
                }
            }
        }
    }

    /// Renders `style` into every loaded spread — visible and preloaded —
    /// and stores it for spreads not served yet. This is the apply path
    /// for everything the Customize panel controls; it never touches the
    /// navigator's preferences.
    private func applyUserStyle(_ style: ReaderUserStyle, anchor: ReaderUserStyle.AnchorMode) {
        userStyleBox.write(style.css())
        guard let pagerSurface else { return }
        // The reflow moves content under any in-flight gesture; stand the
        // pager down and let it re-derive its baselines afterwards, the
        // same contract applyPreferences already follows.
        expectProgrammaticMove = true
        pager?.cancelInteraction()
        let script = style.applyScript(mode: anchor)
        for webView in pagerSurface.loadedWebViews() {
            webView.evaluateJavaScript(script, completionHandler: nil)
        }
        pager?.engageIfNeeded()
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
                try await bookshelf.progress().reportPositionRanges(id: id, counts: counts)
            } catch {
                // The core rejects a breakdown that does not line up with
                // its own spine — it drops duplicate and over-long spine
                // hrefs that Readium's reading order keeps. The per-chapter
                // spans are lost, but "page N of M" need not be.
                logger.warning("Position ranges rejected for \(id, privacy: .public): \(error)")
                guard total > 0 else { return }
                try await bookshelf.progress().reportPositionCount(id: id, count: UInt32(clamping: total))
            }
        }
    }

    private func fetchChapters() {
        Task { [weak self, id = publication.id, logger] in
            do {
                let bookshelf = try await LibraryStore.shared.library()
                let chapters = try await bookshelf.library().chapters(id: id)
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
        // Every navigation shifts the preload window; new spreads arrive
        // with their native gestures enabled and must be re-suppressed.
        pager?.engageIfNeeded()

        if expectProgrammaticMove {
            expectProgrammaticMove = false
        } else if !UIAccessibility.isVoiceOverRunning {
            // Reading takes the page: turning it tucks the chrome away —
            // except under VoiceOver, where the turn came from a scroll
            // action and the chrome is the navigation.
            setChrome(visible: false)
        }

        // A turn VoiceOver asked for announces its new position here, the
        // moment the navigator reports where the page landed.
        if pendingScrollAnnouncement != nil {
            postPageAnnouncement()
        }

        // One `updateProgress` per page turn: opaque locator, book-wide
        // totalProgression, and the synthetic position once known. A live
        // style preview reflows per slider step; the single write that
        // matters lands when the interaction commits.
        guard
            !liveStyleSession,
            let locatorJSON = try? locator.jsonString(),
            let totalProgression = locator.locations.totalProgression
        else { return }
        let position = locator.locations.position.map(UInt32.init)
        enqueueCoreWrite("progress") { [id = publication.id] bookshelf in
            try await bookshelf.progress().updateProgress(
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

    // MARK: Reading band

    /// The shared reading band (`ReaderMetrics`, mirrored by the Android
    /// shell). Readium applies a delegate-provided inset verbatim — it does
    /// not add the safe area on top — so the safe area is folded in here.
    /// Without this, Readium's default table (`max(safeArea, 62)`) swallows
    /// the Dynamic Island and starts the text 3 pt under it.
    func navigatorContentInset(_ navigator: VisualNavigator) -> UIEdgeInsets? {
        ReaderMetrics.contentInsets(
            safeArea: view.safeAreaInsets,
            isPad: traitCollection.userInterfaceIdiom == .pad
        )
    }

    // MARK: Chrome

    func navigator(_ navigator: VisualNavigator, didTapAt point: CGPoint) {
        // Any path that quietly took the chain away — a text selection in
        // the web view, a sheet dismissed by a route with no delegate —
        // is repaired by the next tap, so hardware paging never stays dead
        // for the rest of the session.
        takeKeyCommandChain()
        if let searchPanel, searchPanel.alpha > 0 {
            hideSearch()
        } else if menuVisible {
            setMenu(visible: false)
        } else if let pager, let zone = edgeTapZone(for: point) {
            // Edge taps turn pages, Apple Books style — geometric, like
            // the drags: the right edge always asks for the +x page.
            // A refused turn (the end of the book) is simply nothing
            // happening, exactly as before.
            if zone == .right {
                pager.turnRight()
            } else {
                pager.turnLeft()
            }
        } else {
            setChrome(visible: !chromeVisible)
        }
    }

    private enum EdgeTapZone { case left, right }

    /// The side tap bands: 30% of the width, at least 80 pt — shared
    /// with the Android shell.
    private func edgeTapZone(for point: CGPoint) -> EdgeTapZone? {
        let width = view.bounds.width
        let band = max(width * 0.3, 80)
        if point.x < band { return .left }
        if point.x > width - band { return .right }
        return nil
    }

    /// Hardware keyboard paging, replacing what Readium's directional
    /// adapter used to provide: arrows are geometric, space reads on.
    /// Withheld entirely while the search field owns the keyboard —
    /// guarding here, not in the handlers, so space and arrows reach the
    /// text-input system (CJK composition drives on space) instead of
    /// being swallowed by the priority flag.
    override var canBecomeFirstResponder: Bool { true }

    /// The reader owns the responder chain the key commands are collected
    /// from — Readium's navigator used to take it for its own press
    /// observation and is refused it now (`ReadiumNavigatorShim`).
    ///
    /// It holds the chain only while it is the frontmost thing on screen.
    /// A first responder that is not a text input summons the software
    /// keyboard the moment something enables the scene's focus system, and
    /// every `UIMenu` pull-down does exactly that — so anything presented
    /// over the reader (the Theme & type sheet, whose Font row is such a
    /// pull-down) gets the chain back for as long as it is up.
    private func takeKeyCommandChain() {
        guard presentedViewController == nil, searchPanel?.isEditing != true else { return }
        becomeFirstResponder()
    }

    override func present(
        _ viewControllerToPresent: UIViewController,
        animated flag: Bool,
        completion: (() -> Void)? = nil
    ) {
        _ = resignFirstResponder()
        // Every sheet the reader puts up must hand the key-command chain
        // back when it leaves, and an interactive swipe-down reaches
        // neither `dismiss(animated:)` nor any `onClose` — only the
        // adaptive presentation delegate. Claiming that delegate here, for
        // whatever is presented, closes the whole family in one place; a
        // controller that already installed a delegate of its own keeps it.
        if let presentation = viewControllerToPresent.presentationController,
           presentation.delegate == nil {
            presentation.delegate = self
        }
        super.present(viewControllerToPresent, animated: flag, completion: completion)
    }

    override func dismiss(animated flag: Bool, completion: (() -> Void)? = nil) {
        super.dismiss(animated: flag) { [weak self] in
            completion?()
            self?.takeKeyCommandChain()
        }
    }

    override var keyCommands: [UIKeyCommand]? {
        if searchPanel?.isEditing == true { return nil }
        let commands = [
            UIKeyCommand(input: UIKeyCommand.inputLeftArrow, modifierFlags: [], action: #selector(keyTurnLeft)),
            UIKeyCommand(input: UIKeyCommand.inputRightArrow, modifierFlags: [], action: #selector(keyTurnRight)),
            UIKeyCommand(input: " ", modifierFlags: [], action: #selector(keyTurnForward)),
        ]
        for command in commands {
            command.wantsPriorityOverSystemBehavior = true
        }
        return commands
    }

    /// VoiceOver's three-finger swipe, reached through the shim that stops
    /// Readium's navigator from claiming it first (see
    /// `ReadiumNavigatorShim`); the override below is the path
    /// for anything that does bubble all the way up to the reader.
    ///
    /// Horizontal is geometric, exactly like the edge taps and the arrow
    /// keys: a swipe left asks for the +x page, so reading progression is
    /// honored by the pager rather than re-derived here. `.next`/
    /// `.previous` — and the vertical pair VoiceOver sends for page
    /// up/down — read in reading order, like the space key.
    override func accessibilityScroll(_ direction: UIAccessibilityScrollDirection) -> Bool {
        readerAccessibilityScroll(direction)
    }

    func readerAccessibilityScroll(_ direction: UIAccessibilityScrollDirection) -> Bool {
        guard let pager else { return false }
        let turned: Bool
        switch direction {
        case .left: turned = pager.turnRight()
        case .right: turned = pager.turnLeft()
        case .next, .down: turned = pager.turnForward()
        case .previous, .up: turned = pager.turnBackward()
        default: return false
        }
        // A refusal — the end of the book, a renderer mid-move — is
        // reported as one, so VoiceOver can bubble the gesture instead of
        // announcing a page that never turned.
        guard turned else { return false }
        announcePageAfterTurn()
        return true
    }

    /// Arms the post-turn announcement. The page position is read *after*
    /// the turn settles — `locationDidChange` fires it the moment the
    /// navigator reports where the page landed, and this timer is the
    /// backstop for a location refresh that never arrives.
    private func announcePageAfterTurn() {
        pendingScrollAnnouncement?.cancel()
        pendingScrollAnnouncement = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(700))
            guard !Task.isCancelled else { return }
            self?.postPageAnnouncement()
        }
    }

    private func postPageAnnouncement() {
        pendingScrollAnnouncement?.cancel()
        pendingScrollAnnouncement = nil
        // The argument is the shell's own page line ("12 of 340, 4%") —
        // the same truth the chrome shows.
        UIAccessibility.post(notification: .pageScrolled, argument: pageInfoText())
    }

    @objc private func keyTurnLeft() { pager?.turnLeft() }
    @objc private func keyTurnRight() { pager?.turnRight() }
    @objc private func keyTurnForward() { pager?.turnForward() }

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
                _ = try await bookshelf.library().addBookmark(id: id, locator: locatorJSON, progression: progression)
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
        // A swiped-down page sheet fires neither `dismiss(animated:)` nor
        // `onClose`; the delegate is what brings the key-command chain back.
        sheet.presentationController?.delegate = self
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
        // The jump owns the reader now: a spring, frozen turn, or commit
        // still in flight would keep writing offsets over the new spread.
        pager?.cancelInteraction()
        jumpTask?.cancel()
        jumpTask = Task {
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
        pager?.cancelInteraction()
        navigator.submitPreferences(readerPreferences())
        // Preferences can flip the presentation (and recreate spreads);
        // the reload's locationDidChange re-enforces afterwards too.
        pager?.engageIfNeeded()
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
        let host = ReaderSheetNavigationController(root: sheet)
        sheet.onCustomize = { [weak self, weak host] in
            guard let self, let host else { return }
            host.pushViewController(self.makeCustomizePanel(), animated: true)
        }
        // Belt and braces: whatever route the sheet leaves by — including a
        // swipe-down, which never reaches `onClose` — the live style
        // session must not outlive it, or progress writes stay paused.
        host.presentationController?.delegate = self
        present(host, animated: true)
    }

    /// The Customize panel, wired into the user-stylesheet pipeline:
    /// slider touches open a live session (anchor capture, paused progress
    /// writes), previews restyle without persisting, commits persist and
    /// re-land once.
    private func makeCustomizePanel() -> ReaderCustomizeViewController {
        let settings = AppSettings.shared
        let panel = ReaderCustomizeViewController(
            theme: settings.readingTheme,
            textSize: settings.textSize,
            style: .current,
            fallbackPhrase: String(
                localized: "reader_preview_fallback",
                defaultValue: "The quiet hours belong to the reader."
            ),
            phraseProvider: { [weak self] in await self?.currentPagePhrase() }
        )
        panel.onSessionBegin = { [weak self] style in
            guard let self else { return }
            self.liveStyleSession = true
            self.applyUserStyle(style, anchor: .begin)
        }
        panel.onPreview = { [weak self] style in
            self?.applyUserStyle(style, anchor: .live)
        }
        panel.onCommit = { [weak self] style in
            guard let self else { return }
            style.persist()
            self.liveStyleSession = false
            self.applyUserStyle(style, anchor: .end)
        }
        panel.onClose = { [weak self] in
            guard let self else { return }
            self.liveStyleSession = false
            self.presentedViewController?.dismiss(animated: true)
        }
        return panel
    }

    /// A phrase from the page the reader is looking at, for the Customize
    /// preview: visible text nodes of the current column, segmented
    /// script-agnostically (Intl.Segmenter handles CJK and Thai; the regex
    /// is a defensive fallback), one random pick. Nil on image-only pages.
    private func currentPagePhrase() async -> String? {
        guard let navigator else { return nil }
        let script = """
        (function () {
          var d = document, W = window.innerWidth, H = window.innerHeight;
          if (!d.body) return null;
          function visible(r) {
            var cx = (r.left + r.right) / 2, cy = (r.top + r.bottom) / 2;
            return r.width > 0 && r.height > 0 && cx >= 0 && cx <= W && cy >= 0 && cy <= H;
          }
          var walker = d.createTreeWalker(d.body, NodeFilter.SHOW_TEXT, {
            acceptNode: function (n) {
              if (!n.nodeValue || !n.nodeValue.trim()) return NodeFilter.FILTER_REJECT;
              var p = n.parentElement; if (!p) return NodeFilter.FILTER_REJECT;
              var t = p.tagName;
              if (t === 'SCRIPT' || t === 'STYLE' || t === 'NOSCRIPT' || t === 'RT' || t === 'RP' ||
                  t === 'CODE' || t === 'PRE' || t === 'SUP' || t === 'SUB')
                return NodeFilter.FILTER_REJECT;
              return NodeFilter.FILTER_ACCEPT;
            }
          });
          var parts = [], n, budget = 6000, visited = 0;
          while (budget > 0 && visited++ < 4000 && (n = walker.nextNode())) {
            var r = d.createRange(); r.selectNodeContents(n);
            var rects = r.getClientRects(), hit = false;
            for (var i = 0; i < rects.length; i++) if (visible(rects[i])) { hit = true; break; }
            if (hit) { parts.push(n.nodeValue); budget -= n.nodeValue.length; }
          }
          var text = parts.join(' ').replace(/\\s+/g, ' ').trim();
          if (!text) return null;
          var sentences = [];
          try {
            var seg = new Intl.Segmenter(d.documentElement.lang || undefined, { granularity: 'sentence' });
            var it = seg.segment(text)[Symbol.iterator](), s;
            while (!(s = it.next()).done) sentences.push(s.value.segment.trim());
          } catch (e) {
            sentences = text.split(/(?<=[.!?\\u3002\\uFF01\\uFF1F\\u2026])\\s*/);
          }
          var cjk = (text.match(/[\\u2E80-\\u9FFF\\uF900-\\uFAFF\\uFF66-\\uFF9F\\u3040-\\u30FF\\uAC00-\\uD7AF]/g) || []).length;
          var dense = cjk / text.length > 0.25;
          var lo = dense ? 8 : 24, hi = dense ? 56 : 170;
          // Bounds and the fallback cut count characters, not UTF-16 code
          // units: slicing mid-surrogate would end the phrase in U+FFFD,
          // and emoji and rarer CJK live above the BMP.
          var cands = sentences.filter(function (s) {
            if (!s || /^[\\s\\W_]+$/.test(s)) return false;
            var n = Array.from(s).length;
            return n >= lo && n <= hi;
          });
          if (!cands.length) {
            var chars = Array.from(text.trim());
            return chars.length >= lo ? chars.slice(0, hi).join('') : null;
          }
          return cands[Math.floor(Math.random() * cands.length)];
        })()
        """
        guard case let .success(value) = await navigator.evaluateJavaScript(script) else { return nil }
        guard let phrase = value as? String, !phrase.isEmpty else { return nil }
        return phrase
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
        // The query field owned the chain while the panel was up.
        takeKeyCommandChain()
    }

    /// One in-book search through the core. A failure is not an error the
    /// reader interrupts reading for: the panel shows its empty state and
    /// the reason goes to the log.
    private func runSearch(_ query: String) async -> BookSearchResults? {
        do {
            let bookshelf = try await LibraryStore.shared.library()
            return try await bookshelf.search().searchInBook(
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
        // The jump owns the reader now — same standdown as a contents jump.
        pager?.cancelInteraction()
        jumpTask?.cancel()
        jumpTask = Task {
            _ = await navigator.go(to: target, options: NavigatorGoOptions(animated: false))
        }
    }

    // MARK: Position

    private func applyBrightness(_ brightness: Double) {
        // The design's veil: alpha ramps up as brightness drops below 78%.
        dimView.alpha = max(0, 0.78 - brightness) / 1.7
        // At full brightness the veil contributes nothing; hide it so the
        // compositor provably skips the full-screen blend on every frame.
        dimView.isHidden = dimView.alpha == 0
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
        // that is not a page turn either. A gesture caught mid-rotation is
        // abandoned — the re-layout owns the screen now.
        expectProgrammaticMove = true
        pager?.cancelInteraction()
    }

    /// Fires while each spread web view is being configured — the
    /// earliest tick after Readium creates a new one, whose native
    /// gestures must be suppressed before it can claim a touch.
    func navigator(_ navigator: EPUBNavigatorViewController, setupUserScripts userContentController: WKUserContentController) {
        DispatchQueue.main.async { [weak self] in
            self?.pager?.engageIfNeeded()
        }
    }
}

extension ReaderViewController: UIAdaptivePresentationControllerDelegate {
    /// The Theme & type sheet went away by swipe-down. A live style
    /// session can never outlive the panel that opened it, or progress
    /// writes would stay paused for the rest of the reading session.
    func presentationControllerDidDismiss(_ presentationController: UIPresentationController) {
        liveStyleSession = false
        // The swipe-down never routes through `dismiss(animated:)`, so the
        // key-command chain is taken back here too.
        takeKeyCommandChain()
    }
}
