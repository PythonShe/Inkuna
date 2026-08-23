import os
import UIKit

@MainActor final class ReadingSessionBox { var id: String? }

/// UIKit shell for the core reader engine.
@MainActor
final class ReaderViewController: UIViewController {
    enum LayoutEvent {
        case first(UInt64, UInt32)
        case complete(UInt64, UInt32, UInt32)
        case failed(UInt64, UInt32)

        var generation: UInt64 {
            switch self {
            case let .first(generation, _), let .complete(generation, _, _), let .failed(generation, _): generation
            }
        }
    }

    let publication: Publication
    let initialChapter: Chapter?
    var readerSession: ReaderSession?
    var layoutRelay: ReaderLayoutRelay?
    var canvas: EnginePageCanvas?
    var pagerSurface: EnginePagerSurface?
    var pager: ReaderPager?
    var selectionController: ReaderSelectionController?
    var chapters: [Chapter] = []
    var chapterRanges: [ChapterPositionRange] = []
    var layoutGeneration: UInt64?
    var targetCoordinate: Coordinate?
    var relayoutAnchor: Coordinate?
    var pendingEvents: [LayoutEvent] = []
    var layoutChangeInFlight = false
    var generationBeforeLayout: UInt64?
    var didLogFirstRender = false
    var announcePageWhenSettled = false
    var didLogChapterComplete = false
    var openedAt: Date?
    var firstPageReadyAt: Date?
    let session = ReadingSessionBox()
    var coreWriteChain: Task<Void, Never>?
    var openTask: Task<Void, Never>?
    var canvasTop: NSLayoutConstraint?
    var canvasBottom: NSLayoutConstraint?
    let logger = Logger(subsystem: "app.inkuna.ios", category: "reader")
    let perfLogger = Logger(subsystem: "app.inkuna.ios", category: "perf")

    let loadingIndicator = UIActivityIndicatorView(style: .medium)
    let openFailureLabel = InkLabel()
    let dimView = UIView()
    let pageInfoLabel = InkLabel()
    let bookmarkFeedback = UIImpactFeedbackGenerator(style: .light)
    lazy var backButton = ReaderGlassButton(symbol: "arrow.backward", accessibilityLabel: String(localized: "a11y_back", defaultValue: "Back")) { [weak self] in self?.navigationController?.popViewController(animated: true) }
    lazy var menuButton = ReaderGlassButton(symbol: "ellipsis", pointSize: 19, accessibilityLabel: String(localized: "a11y_reading_menu", defaultValue: "Reading menu")) { [weak self] in
        guard let self else { return }
        self.setMenu(visible: !self.menuVisible)
    }
    lazy var menuView = ReaderMenuView(
        onContents: { [weak self] in self?.setMenu(visible: false); self?.presentContents() },
        onTheme: { [weak self] in self?.setMenu(visible: false); self?.presentThemeSheet() },
        onSearch: { [weak self] in self?.setMenu(visible: false); self?.showSearch() },
        onBookmark: { [weak self] in self?.placeBookmark() }
    )
    var menuVisible = false
    var menuAnimator: UIViewPropertyAnimator?
    var searchPanel: ReaderSearchPanel?
    var chromeVisible = true
    var chromeAnimator: UIViewPropertyAnimator?

    init(publication: Publication, initialChapter: Chapter? = nil) {
        self.publication = publication
        self.initialChapter = initialChapter
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    deinit {
        openTask?.cancel()
        NotificationCenter.default.removeObserver(self)
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        installShell(theme: AppSettings.shared.readingTheme)
        updatePageInfo()
        bookmarkFeedback.prepare()
        NotificationCenter.default.addObserver(self, selector: #selector(appDidEnterBackground), name: UIApplication.didEnterBackgroundNotification, object: nil)
        NotificationCenter.default.addObserver(self, selector: #selector(appWillEnterForeground), name: UIApplication.willEnterForegroundNotification, object: nil)
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        takeKeyCommandChain()
        startSession()
        if openTask == nil { openTask = Task { await openReader() } }
        #if DEBUG
        runDebugRouteIfNeeded()
        #endif
    }

    override func viewDidDisappear(_ animated: Bool) {
        super.viewDidDisappear(animated)
        endSession()
        if isMovingFromParent || isBeingDismissed {
            selectionController?.clear()
            openTask?.cancel()
        }
    }

    override func viewSafeAreaInsetsDidChange() {
        super.viewSafeAreaInsetsDidChange()
        updateReadingBand()
    }

    @objc func appDidEnterBackground() { endSession() }
    @objc func appWillEnterForeground() { if viewIfLoaded?.window != nil { startSession() } }

    func installShell(theme: ReadingTheme) {
        view.backgroundColor = theme.background
        loadingIndicator.color = theme.dimmedForeground
        loadingIndicator.hidesWhenStopped = true
        loadingIndicator.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(loadingIndicator)
        loadingIndicator.startAnimating()

        openFailureLabel.text = String(localized: "reader_open_failed", defaultValue: "This book could not be opened.")
        openFailureLabel.font = InkFont.reading()
        openFailureLabel.textColor = theme.dimmedForeground
        openFailureLabel.textAlignment = .center
        openFailureLabel.numberOfLines = 0
        openFailureLabel.isHidden = true
        openFailureLabel.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(openFailureLabel)

        dimView.backgroundColor = UIColor(ink: 0x0A0907)
        dimView.isUserInteractionEnabled = false
        dimView.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(dimView)
        applyBrightness(AppSettings.shared.brightness)

        pageInfoLabel.font = InkFont.caption
        pageInfoLabel.textColor = theme.dimmedForeground
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
            dimView.leadingAnchor.constraint(equalTo: view.leadingAnchor), dimView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            dimView.topAnchor.constraint(equalTo: view.topAnchor), dimView.bottomAnchor.constraint(equalTo: view.bottomAnchor),
            backButton.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: InkSpacing.space4),
            backButton.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: 6),
            menuButton.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -InkSpacing.space4),
            menuButton.bottomAnchor.constraint(equalTo: view.safeAreaLayoutGuide.bottomAnchor, constant: -26),
            menuView.trailingAnchor.constraint(equalTo: menuButton.trailingAnchor), menuView.bottomAnchor.constraint(equalTo: menuButton.topAnchor, constant: -12),
            menuView.leadingAnchor.constraint(greaterThanOrEqualTo: view.leadingAnchor, constant: InkSpacing.space4),
            pageInfoLabel.centerXAnchor.constraint(equalTo: view.centerXAnchor),
            pageInfoLabel.bottomAnchor.constraint(equalTo: view.safeAreaLayoutGuide.bottomAnchor, constant: -ReaderMetrics.footerLift),
        ])
    }
}
