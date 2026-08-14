import UIKit

/// The reading screen: horizontally paged prose on the selected page
/// theme, chrome bars that fade away on tap, a bookmark toast, and the
/// Theme & type sheet.
///
/// TODO(core): page content, pagination, and positions come from the Rust
/// core + Readium's `EPUBNavigatorViewController` once rendering lands;
/// this screen reads the placeholder chapter so the experience is real
/// end to end.
final class ReaderViewController: UIViewController, UIScrollViewDelegate {
    private let book: PlaceholderBook

    private let scrollView = UIScrollView()
    private let pageStack = UIStackView()
    private var pageLabels: [UILabel] = []
    private let dimView = UIView()
    private let topBar = ReaderChromeBar()
    private let bottomBar = ReaderChromeBar()
    private let pageInfoLabel = UILabel()
    private var chromeVisible = true
    private var pageIndex = 0
    private let bookmarkFeedback = UIImpactFeedbackGenerator(style: .light)

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

        for pageNumber in 0..<PlaceholderLibrary.samplePages.count {
            let page = UIView()
            let label = UILabel()
            label.numberOfLines = 0
            label.translatesAutoresizingMaskIntoConstraints = false
            page.addSubview(label)
            pageStack.addArrangedSubview(page)
            NSLayoutConstraint.activate([
                // Below the chrome bar regardless of device safe areas.
                label.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: 64),
                label.centerXAnchor.constraint(equalTo: page.centerXAnchor),
                label.leadingAnchor.constraint(greaterThanOrEqualTo: page.leadingAnchor, constant: 26),
                label.widthAnchor.constraint(lessThanOrEqualToConstant: InkFont.readingMeasure),
            ])
            // High, not required: overrunning prose extends beneath the
            // translucent bottom bar instead of clipping mid-sentence.
            // TODO(core): real pagination via Readium removes overflow.
            let bottom = label.bottomAnchor.constraint(
                lessThanOrEqualTo: view.safeAreaLayoutGuide.bottomAnchor,
                constant: -70
            )
            bottom.priority = .defaultHigh
            bottom.isActive = true
            pageLabels.append(label)
            page.widthAnchor.constraint(equalTo: scrollView.frameLayoutGuide.widthAnchor).isActive = true
            _ = pageNumber
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

        // MARK: Chrome

        // TODO(l10n): localize once the strings pass lands.
        let backButton = InkIconButton(symbol: "chevron.backward", accessibilityLabel: "Back") { [weak self] in
            self?.navigationController?.popViewController(animated: true)
        }
        let chapterLabel = UILabel()
        chapterLabel.text = PlaceholderLibrary.pagesLeftText
        chapterLabel.font = InkFont.caption
        chapterLabel.textColor = InkColor.textTertiary
        let bookmarkButton = InkIconButton(symbol: "bookmark", accessibilityLabel: "Bookmark") { [weak self] in
            self?.placeBookmark()
        }
        topBar.fill(leading: backButton, center: chapterLabel, trailing: bookmarkButton)

        let contentsButton = InkIconButton(symbol: "list.bullet", accessibilityLabel: "Contents") { [weak self] in
            self?.showContents()
        }
        pageInfoLabel.font = InkFont.caption
        pageInfoLabel.textColor = InkColor.textTertiary
        let typeButton = InkIconButton(symbol: "textformat.size", accessibilityLabel: "Theme and type") { [weak self] in
            self?.presentThemeSheet()
        }
        bottomBar.fill(leading: contentsButton, center: pageInfoLabel, trailing: typeButton)
        updatePageInfo()

        for bar in [topBar, bottomBar] {
            bar.translatesAutoresizingMaskIntoConstraints = false
            view.addSubview(bar)
        }
        NSLayoutConstraint.activate([
            topBar.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            topBar.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            topBar.topAnchor.constraint(equalTo: view.topAnchor),
            bottomBar.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            bottomBar.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            bottomBar.bottomAnchor.constraint(equalTo: view.bottomAnchor),
        ])

        let tap = UITapGestureRecognizer(target: self, action: #selector(handleTap))
        view.addGestureRecognizer(tap)
    }

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

    // MARK: Chrome behavior

    @objc private func handleTap() {
        chromeVisible.toggle()
        let visible = chromeVisible
        for bar in [topBar, bottomBar] {
            bar.isUserInteractionEnabled = visible
        }
        InkMotion.runQuiet {
            self.topBar.alpha = visible ? 1 : 0
            self.bottomBar.alpha = visible ? 1 : 0
        }
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

    private func showContents() {
        guard let navigationController else { return }
        if let detail = navigationController.viewControllers.last(where: { $0 is BookDetailViewController }) {
            navigationController.popToViewController(detail, animated: true)
        } else {
            // Keep the reader beneath so backing out of the contents
            // returns to the open page.
            let detail = BookDetailViewController(book: book)
            detail.hidesBottomBarWhenPushed = true
            navigationController.pushViewController(detail, animated: true)
        }
    }

    private func presentThemeSheet() {
        let sheet = ThemeTypeSheetViewController()
        sheet.onThemeChange = { [weak self] theme in
            AppSettings.shared.readingTheme = theme
            guard let self else { return }
            InkMotion.runQuiet {
                self.view.backgroundColor = theme.background
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

    private func applyBrightness(_ brightness: Double) {
        // The design's veil: alpha ramps up as brightness drops below 78%.
        dimView.alpha = max(0, 0.78 - brightness) / 1.7
    }

    private func updatePageInfo() {
        // TODO(core): live position from the core's progress tracking.
        let page = book.currentPage + pageIndex
        let percent = Int((Double(page) / Double(book.pageCount) * 100).rounded())
        pageInfoLabel.text = "p. \(page) of \(book.pageCount) · \(percent)%"
    }

    // MARK: UIScrollViewDelegate

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

/// Translucent chrome bar for the reader: surface-tinted material with a
/// three-slot layout.
private final class ReaderChromeBar: UIView {
    private let effectView = UIVisualEffectView(effect: UIBlurEffect(style: .systemThinMaterial))
    private let tint = UIView()

    init() {
        super.init(frame: .zero)
        effectView.translatesAutoresizingMaskIntoConstraints = false
        addSubview(effectView)
        tint.backgroundColor = InkColor.bgSurface.withAlphaComponent(0.6)
        tint.translatesAutoresizingMaskIntoConstraints = false
        addSubview(tint)
        NSLayoutConstraint.activate([
            effectView.leadingAnchor.constraint(equalTo: leadingAnchor),
            effectView.trailingAnchor.constraint(equalTo: trailingAnchor),
            effectView.topAnchor.constraint(equalTo: topAnchor),
            effectView.bottomAnchor.constraint(equalTo: bottomAnchor),
            tint.leadingAnchor.constraint(equalTo: leadingAnchor),
            tint.trailingAnchor.constraint(equalTo: trailingAnchor),
            tint.topAnchor.constraint(equalTo: topAnchor),
            tint.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    func fill(leading: UIView, center: UIView, trailing: UIView) {
        let row = UIStackView(arrangedSubviews: [leading, UIView(), center, UIView(), trailing])
        row.axis = .horizontal
        row.alignment = .center
        row.distribution = .equalCentering
        row.translatesAutoresizingMaskIntoConstraints = false
        addSubview(row)
        NSLayoutConstraint.activate([
            row.leadingAnchor.constraint(equalTo: leadingAnchor, constant: InkSpacing.space3),
            row.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -InkSpacing.space3),
            row.topAnchor.constraint(equalTo: safeAreaLayoutGuide.topAnchor, constant: 10),
            row.bottomAnchor.constraint(equalTo: safeAreaLayoutGuide.bottomAnchor, constant: -10),
        ])
    }
}

private extension UIFont {
    func withItalics() -> UIFont {
        guard let descriptor = fontDescriptor.withSymbolicTraits([.traitItalic]) else { return self }
        return UIFont(descriptor: descriptor, size: pointSize)
    }
}
