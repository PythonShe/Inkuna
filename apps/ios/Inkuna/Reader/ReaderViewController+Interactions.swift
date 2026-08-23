import UIKit

extension ReaderViewController {
    func handleCanvasTap(_ point: CGPoint) {
        takeKeyCommandChain()
        if let selectionController, selectionController.isActive {
            if !selectionController.containsSelection(at: point) { selectionController.clear() }
            return
        }
        if let searchPanel, searchPanel.alpha > 0 { hideSearch(); return }
        if menuVisible { setMenu(visible: false); return }
        guard let readerSession, let surface = pagerSurface,
              let pagePoint = surface.pagePoint(fromCanvasPoint: point),
              let hit = try? readerSession.hitTest(
                spineIdx: surface.spineIdx,
                pageIdx: surface.pageIdx,
                x: pagePoint.x,
                y: pagePoint.y
              ),
              let target = hit.linkTarget else {
            handleNonLinkTap(point)
            return
        }
        followLink(target)
    }

    func followLink(_ target: String) {
        if let url = URL(string: target), let scheme = url.scheme?.lowercased() {
            guard scheme == "http" || scheme == "https" else { showLinkNotFollowed(); return }
            UIApplication.shared.open(url) { [weak self] in if !$0 { self?.showLinkNotFollowed() } }
            return
        }
        do { try jump(to: resolveHref(target)) }
        catch InkunaError.AnchorNotFound { showLinkNotFollowed() }
        catch { logger.warning("Internal link \(target, privacy: .public) failed: \(error)") }
    }

    func handleNonLinkTap(_ point: CGPoint) {
        if let pager, let zone = edgeTapZone(for: point) {
            _ = zone == .right ? pager.turnRight() : pager.turnLeft()
        } else {
            setChrome(visible: !chromeVisible)
        }
    }

    enum EdgeTapZone { case left, right }
    func edgeTapZone(for point: CGPoint) -> EdgeTapZone? {
        let band = max(view.bounds.width * 0.3, 80)
        if point.x < band { return .left }
        if point.x > view.bounds.width - band { return .right }
        return nil
    }

    func jump(to coordinate: Coordinate) throws {
        guard let readerSession, let surface = pagerSurface else { return }
        let location = try readerSession.locate(coordinate: coordinate)
        guard accept(generation: location.generation) else { return }
        selectionController?.clear()
        pager?.cancelInteraction()
        surface.display(spineIdx: location.spineIdx, pageIdx: location.pageIdx)
    }

    func jump(to chapter: Chapter) {
        do { try jump(to: resolveHref(chapter)) }
        catch InkunaError.AnchorNotFound { showLinkNotFollowed() }
        catch { logger.warning("TOC jump for \(chapter.id, privacy: .public) failed: \(error)") }
    }

    /// Legacy bookmark rows lack a coordinate. The generated session has no
    /// progression-to-coordinate lookup, so those rows remain non-jumpable
    /// until the core rebaseline supplies their coordinate.
    func jump(to bookmark: Bookmark) {
        guard let coordinate = bookmark.coordinate else { showLinkNotFollowed(); return }
        do { try jump(to: coordinate) }
        catch { logger.warning("Bookmark jump failed: \(error)") }
    }

    func setMenu(visible: Bool) {
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
        if visible { UIAccessibility.post(notification: .layoutChanged, argument: menuView) }
    }

    func setChrome(visible: Bool) {
        guard chromeVisible != visible else { return }
        chromeVisible = visible
        if !visible { setMenu(visible: false) }
        [backButton, menuButton].forEach { $0.isUserInteractionEnabled = visible }
        chromeAnimator?.stopAnimation(true)
        let animator = InkMotion.quietAnimator(duration: 0.24)
        animator.addAnimations {
            [self.backButton, self.menuButton, self.pageInfoLabel].forEach { $0.alpha = visible ? 1 : 0 }
        }
        animator.startAnimation()
        chromeAnimator = animator
    }

    func placeBookmark() {
        guard let coordinate = currentAnchor() else {
            InkToastView.show(symbol: "bookmark.slash", text: String(localized: "reader_bookmark_empty", defaultValue: "Nothing to bookmark yet."), in: view, topInset: view.safeAreaInsets.top + 56)
            return
        }
        bookmarkFeedback.impactOccurred()
        Task { [weak self, id = publication.id, logger] in
            do {
                let shelf = try await LibraryStore.shared.library()
                let progression = try await ReaderPositions.progression(of: coordinate, id: id, on: shelf)
                _ = try await shelf.library().addBookmark(id: id, coordinate: coordinate, progression: progression)
                guard let self else { return }
                InkToastView.show(symbol: "bookmark.fill", text: String(localized: "reader_bookmark_placed", defaultValue: "Bookmark placed."), in: self.view, topInset: self.view.safeAreaInsets.top + 56)
            } catch {
                logger.warning("Bookmark for \(id, privacy: .public) failed: \(error)")
            }
        }
    }

    func presentContents() {
        let currentPosition: UInt32? = currentAnchor().flatMap { readerSession?.positionOf(coordinate: $0) }
        let currentChapter = currentPosition.flatMap { ReaderPositions.chapterRange(in: chapterRanges, at: $0)?.chapterIdx }
        let rows = chapters.map { chapter in
            let range = chapterRanges.first { $0.chapterIdx == chapter.idx }
            return ContentsSheetViewController.Row(chapter: chapter, position: range.map { Int($0.startPosition) }, isCurrent: chapter.idx == currentChapter)
        }
        let sheet = ContentsSheetViewController(
            bookTitle: publication.title,
            coverSeed: BookCoverView.coverSeed(for: publication.id),
            coverPath: publication.coverPath,
            rows: rows,
            pageInfoText: pageInfoText()
        )
        sheet.onSelectChapter = { [weak self] in self?.jump(to: $0) }
        present(sheet, animated: true)
    }

    func presentThemeSheet() {
        let sheet = ThemeTypeSheetViewController()
        sheet.onThemeChange = { [weak self] theme in
            AppSettings.shared.readingTheme = theme
            self?.applyTheme(theme)
        }
        sheet.onSizeChange = { [weak self] size in
            AppSettings.shared.textSize = size
            Task { @MainActor in await self?.relayout(anchor: self?.currentAnchor()) }
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
        present(host, animated: true)
    }

    func makeCustomizePanel() -> ReaderCustomizeViewController {
        let settings = AppSettings.shared
        let panel = ReaderCustomizeViewController(
            theme: settings.readingTheme,
            textSize: settings.textSize,
            fallbackPhrase: String(localized: "reader_preview_fallback", defaultValue: "The quiet hours belong to the reader."),
            phraseProvider: { nil }
        )
        panel.onSessionBegin = { [weak self] in self?.relayoutAnchor = self?.currentAnchor() }
        panel.onCommit = { [weak self] in
            Task { @MainActor in await self?.relayout(anchor: self?.relayoutAnchor) }
        }
        panel.onClose = { [weak self] in self?.presentedViewController?.dismiss(animated: true) }
        return panel
    }

    func applyTheme(_ theme: ReadingTheme) {
        canvas?.theme = theme
        selectionController?.updateTheme()
        InkMotion.runQuiet {
            self.view.backgroundColor = theme.background
            self.pageInfoLabel.textColor = theme.dimmedForeground
            self.loadingIndicator.color = theme.dimmedForeground
            self.openFailureLabel.textColor = theme.dimmedForeground
        }
    }

    func showSearch() {
        let panel: ReaderSearchPanel
        if let existing = searchPanel {
            panel = existing
        } else {
            panel = ReaderSearchPanel(
                search: { [weak self] query in await self?.runSearch(query) },
                positionForHit: { [weak self] hit in
                    self?.readerSession.map {
                        Int($0.positionOf(coordinate: Coordinate(spineIdx: hit.spineIdx, charOffset: UInt64(hit.charOffset))))
                    }
                }
            )
            panel.onClose = { [weak self] in self?.hideSearch() }
            panel.onJump = { [weak self] in self?.jump(to: $0) }
            panel.alpha = 0
            panel.accessibilityViewIsModal = true
            panel.translatesAutoresizingMaskIntoConstraints = false
            view.addSubview(panel)
            NSLayoutConstraint.activate([
                panel.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 14),
                panel.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -14),
                panel.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: 8),
                panel.bottomAnchor.constraint(lessThanOrEqualTo: view.keyboardLayoutGuide.topAnchor, constant: -12),
            ])
            searchPanel = panel
        }
        setChrome(visible: false)
        InkMotion.runQuiet { panel.alpha = 1 }
        panel.focus()
        UIAccessibility.post(notification: .layoutChanged, argument: panel)
    }

    func hideSearch() {
        guard let panel = searchPanel else { return }
        panel.endEditing(true)
        let animator = InkMotion.quietAnimator(duration: InkMotion.fast)
        animator.addAnimations { panel.alpha = 0 }
        animator.addCompletion { _ in panel.reset() }
        animator.startAnimation()
        setChrome(visible: true)
        takeKeyCommandChain()
    }

    func runSearch(_ query: String) async -> BookSearchResults? {
        do {
            let shelf = try await LibraryStore.shared.library()
            return try await shelf.search().searchInBook(id: publication.id, query: query, limit: ReaderSearchPanel.hitLimit)
        } catch {
            self.logger.warning("Search in \(self.publication.id, privacy: .public) failed: \(error)")
            return nil
        }
    }

    func jump(to hit: BookSearchHit) {
        do {
            try jump(to: Coordinate(spineIdx: hit.spineIdx, charOffset: UInt64(hit.charOffset)))
            hideSearch()
        } catch {
            logger.warning("Search jump failed: \(error)")
        }
    }
}
