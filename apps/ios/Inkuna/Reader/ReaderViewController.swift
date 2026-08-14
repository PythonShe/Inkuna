import UIKit

/// The reading screen: horizontally paged prose on the selected page theme
/// under floating glass chrome — a back button, the page line, and a more
/// button that fans out the reading menu (contents, theme & type, in-book
/// search, bookmark). The chrome shows on entry, tucks away when a page is
/// turned, and comes back on tap.
///
/// TODO(core): page content, pagination, and positions come from the Rust
/// core + Readium's `EPUBNavigatorViewController` once rendering lands;
/// this screen reads the placeholder chapter so the experience is real
/// end to end.
final class ReaderViewController: UIViewController, UIScrollViewDelegate, UIGestureRecognizerDelegate {
    private let book: PlaceholderBook

    private let scrollView = UIScrollView()
    private let pageStack = UIStackView()
    private var pageLabels: [UILabel] = []
    private let dimView = UIView()
    private let pageInfoLabel = InkLabel()
    private var pageIndex = 0
    private let bookmarkFeedback = UIImpactFeedbackGenerator(style: .light)

    // MARK: Floating chrome

    private lazy var backButton = ReaderGlassButton(symbol: "arrow.backward", accessibilityLabel: "Back") { [weak self] in
        self?.navigationController?.popViewController(animated: true)
    }
    private lazy var menuButton = ReaderGlassButton(symbol: "ellipsis", pointSize: 19, accessibilityLabel: "Reading menu") { [weak self] in
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
    /// while reading (page swipes hide it, taps toggle it).
    private var chromeVisible = true
    private var chromeAnimator: UIViewPropertyAnimator?

    init(book: PlaceholderBook) {
        self.book = book
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    override func viewDidLoad() {
        super.viewDidLoad()
        let settings = AppSettings.shared
        view.backgroundColor = settings.readingTheme.background

        // MARK: Pages

        scrollView.isPagingEnabled = true
        scrollView.showsHorizontalScrollIndicator = false
        scrollView.contentInsetAdjustmentBehavior = .never
        scrollView.delegate = self
        scrollView.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(scrollView)

        pageStack.axis = .horizontal
        pageStack.translatesAutoresizingMaskIntoConstraints = false
        scrollView.addSubview(pageStack)

        NSLayoutConstraint.activate([
            scrollView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            scrollView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            scrollView.topAnchor.constraint(equalTo: view.topAnchor),
            scrollView.bottomAnchor.constraint(equalTo: view.bottomAnchor),
            pageStack.leadingAnchor.constraint(equalTo: scrollView.contentLayoutGuide.leadingAnchor),
            pageStack.trailingAnchor.constraint(equalTo: scrollView.contentLayoutGuide.trailingAnchor),
            pageStack.topAnchor.constraint(equalTo: scrollView.contentLayoutGuide.topAnchor),
            pageStack.bottomAnchor.constraint(equalTo: scrollView.contentLayoutGuide.bottomAnchor),
            pageStack.heightAnchor.constraint(equalTo: scrollView.frameLayoutGuide.heightAnchor),
        ])

        for _ in 0..<PlaceholderLibrary.samplePages.count {
            let page = UIView()
            let label = InkLabel()
            label.numberOfLines = 0
            label.translatesAutoresizingMaskIntoConstraints = false
            page.addSubview(label)
            pageStack.addArrangedSubview(page)
            NSLayoutConstraint.activate([
                // Clear of the floating back button on any device.
                label.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: 64),
                label.centerXAnchor.constraint(equalTo: page.centerXAnchor),
                label.leadingAnchor.constraint(greaterThanOrEqualTo: page.leadingAnchor, constant: 26),
                label.widthAnchor.constraint(lessThanOrEqualToConstant: InkFont.readingMeasure),
            ])
            // High, not required: overrunning prose extends toward the page
            // edge instead of clipping mid-sentence.
            // TODO(core): real pagination via Readium removes overflow.
            let bottom = label.bottomAnchor.constraint(
                lessThanOrEqualTo: view.safeAreaLayoutGuide.bottomAnchor,
                constant: -70
            )
            bottom.priority = .defaultHigh
            bottom.isActive = true
            pageLabels.append(label)
            page.widthAnchor.constraint(equalTo: scrollView.frameLayoutGuide.widthAnchor).isActive = true
        }
        renderPages()

        // MARK: Brightness veil

        dimView.backgroundColor = UIColor(ink: 0x0A0907)
        dimView.isUserInteractionEnabled = false
        dimView.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(dimView)
        NSLayoutConstraint.activate([
            dimView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            dimView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            dimView.topAnchor.constraint(equalTo: view.topAnchor),
            dimView.bottomAnchor.constraint(equalTo: view.bottomAnchor),
        ])
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

        let tap = UITapGestureRecognizer(target: self, action: #selector(handleTap))
        tap.delegate = self
        view.addGestureRecognizer(tap)

        bookmarkFeedback.prepare()
    }

    #if DEBUG
    // Deep-launch a chrome state for the screenshot loop:
    // `-inkuna.debugScreen reader -inkuna.debugReaderUI menu|search|contents|theme`
    private var didRunDebugRoute = false

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        guard !didRunDebugRoute else { return }
        didRunDebugRoute = true
        switch UserDefaults.standard.string(forKey: "inkuna.debugReaderUI") {
        case "menu": setMenu(visible: true)
        case "search": showSearch()
        case "contents": presentContents()
        case "theme": presentThemeSheet()
        case "immersed": setChrome(visible: false)
        default: break
        }
    }
    #endif

    // MARK: Prose

    private func renderPages() {
        let settings = AppSettings.shared
        let theme = settings.readingTheme
        let font = InkFont.reading(settings.textSize)

        let paragraphStyle = NSMutableParagraphStyle()
        let lineHeight = font.pointSize * InkFont.readingLineHeightMultiple
        paragraphStyle.minimumLineHeight = lineHeight
        paragraphStyle.maximumLineHeight = lineHeight
        paragraphStyle.paragraphSpacing = font.pointSize * 0.9

        for (index, paragraphs) in PlaceholderLibrary.samplePages.enumerated() {
            let text = NSMutableAttributedString()
            if index == 0 {
                let eyebrowFont = InkFont.caption
                text.append(
                    NSAttributedString(
                        string: PlaceholderLibrary.chapterEyebrow.uppercased() + "\n",
                        attributes: [
                            .font: eyebrowFont,
                            .foregroundColor: theme.dimmedForeground,
                            .kern: InkFont.capsKern(for: eyebrowFont),
                            .paragraphStyle: paragraphStyle,
                        ]
                    )
                )
            }
            let isLastPage = index == PlaceholderLibrary.samplePages.count - 1
            for (paragraphIndex, paragraph) in paragraphs.enumerated() {
                let isRestHere = isLastPage && paragraphIndex == paragraphs.count - 1
                let bodyFont = isRestHere
                    ? InkFont.serif(font.pointSize, weight: .regular, style: .body).withItalics()
                    : font
                let terminator = paragraphIndex == paragraphs.count - 1 ? "" : "\n"
                text.append(
                    NSAttributedString(
                        string: paragraph + terminator,
                        attributes: [
                            .font: bodyFont,
                            .foregroundColor: isRestHere
                                ? theme.foreground.withAlphaComponent(0.55)
                                : theme.foreground,
                            .paragraphStyle: paragraphStyle,
                        ]
                    )
                )
            }
            pageLabels[index].attributedText = text
        }
    }

    // MARK: Reading menu

    private func setMenu(visible: Bool) {
        guard menuVisible != visible else { return }
        menuVisible = visible
        menuButton.setSymbol(visible ? "xmark" : "ellipsis")
        // TODO(l10n): localize once the strings pass lands.
        menuButton.accessibilityLabel = visible ? "Close reading menu" : "Reading menu"
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

    @objc private func handleTap() {
        if let searchPanel, searchPanel.alpha > 0 {
            hideSearch()
        } else if menuVisible {
            setMenu(visible: false)
        } else {
            setChrome(visible: !chromeVisible)
        }
    }

    func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool {
        // Bare page taps only — chrome and overlays handle their own touches.
        var candidate = touch.view
        while let current = candidate, current !== view {
            if current === menuView || current === backButton || current === menuButton || current === searchPanel {
                return false
            }
            candidate = current.superview
        }
        return true
    }

    private func placeBookmark() {
        // TODO(core): persist the bookmark through the Rust core.
        bookmarkFeedback.impactOccurred()
        InkToastView.show(
            symbol: "bookmark.fill",
            text: "Bookmark placed.",
            in: view,
            topInset: view.safeAreaInsets.top + 56
        )
    }

    private func presentContents() {
        let sheet = ContentsSheetViewController(book: book, pageInfoText: pageInfoText())
        sheet.onSelectChapter = { _ in
            // TODO(core): jump to the chapter position via Readium; the
            // placeholder chapter has nowhere to go yet.
        }
        present(sheet, animated: true)
    }

    private func presentThemeSheet() {
        let sheet = ThemeTypeSheetViewController()
        sheet.onThemeChange = { [weak self] theme in
            AppSettings.shared.readingTheme = theme
            guard let self else { return }
            InkMotion.runQuiet {
                self.view.backgroundColor = theme.background
                self.pageInfoLabel.textColor = theme.dimmedForeground
            }
            self.renderPages()
        }
        sheet.onSizeChange = { [weak self] size in
            AppSettings.shared.textSize = size
            self?.renderPages()
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
            panel = ReaderSearchPanel(basePage: book.currentPage)
            panel.onClose = { [weak self] in self?.hideSearch() }
            panel.onJump = { [weak self] index in
                guard let self else { return }
                self.scrollView.setContentOffset(
                    CGPoint(x: CGFloat(index) * self.scrollView.bounds.width, y: 0),
                    animated: false
                )
                self.pageIndex = index
                self.updatePageInfo()
                self.hideSearch()
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

    // MARK: Position

    private func applyBrightness(_ brightness: Double) {
        // The design's veil: alpha ramps up as brightness drops below 78%.
        dimView.alpha = max(0, 0.78 - brightness) / 1.7
    }

    private func pageInfoText() -> String {
        let page = book.currentPage + pageIndex
        let percent = Int((Double(page) / Double(book.pageCount) * 100).rounded())
        return "p. \(page) of \(book.pageCount) · \(percent)%"
    }

    private func updatePageInfo() {
        // TODO(core): live position from the core's progress tracking.
        pageInfoLabel.text = pageInfoText()
        let page = book.currentPage + pageIndex
        let percent = Int((Double(page) / Double(book.pageCount) * 100).rounded())
        menuView.contentsPill.text = "Contents · \(percent)%"
    }

    // MARK: UIScrollViewDelegate

    func scrollViewWillBeginDragging(_ scrollView: UIScrollView) {
        // Reading takes the page: turning it tucks the chrome away.
        setChrome(visible: false)
    }

    func scrollViewDidEndDecelerating(_ scrollView: UIScrollView) {
        syncPageIndex()
    }

    func scrollViewDidEndDragging(_ scrollView: UIScrollView, willDecelerate decelerate: Bool) {
        if !decelerate {
            syncPageIndex()
        }
    }

    private func syncPageIndex() {
        let width = scrollView.bounds.width
        guard width > 0 else { return }
        let index = Int((scrollView.contentOffset.x / width).rounded())
        guard index != pageIndex else { return }
        pageIndex = index
        updatePageInfo()
    }

    override func viewWillTransition(to size: CGSize, with coordinator: any UIViewControllerTransitionCoordinator) {
        super.viewWillTransition(to: size, with: coordinator)
        // Keep the reader on its page across rotation and window resizes.
        let index = pageIndex
        coordinator.animate(alongsideTransition: nil) { [weak self] _ in
            guard let self else { return }
            self.scrollView.setContentOffset(
                CGPoint(x: CGFloat(index) * self.scrollView.bounds.width, y: 0),
                animated: false
            )
        }
    }
}

private extension UIFont {
    func withItalics() -> UIFont {
        guard let descriptor = fontDescriptor.withSymbolicTraits([.traitItalic]) else { return self }
        return UIFont(descriptor: descriptor, size: pointSize)
    }
}
