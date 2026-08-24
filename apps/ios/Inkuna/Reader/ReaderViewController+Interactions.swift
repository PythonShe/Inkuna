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

    /// Accessibility activation of a link block. Its point arrives in the
    /// core's own page-local space — layout points at 1× with y growing
    /// downward, straight off `A11yBlock.rect` — which is exactly the space
    /// `hitTest` takes, so it needs no view-space conversion (unlike
    /// `handleCanvasTap`, whose point starts out canvas-local).
    func activateLink(spineIdx: UInt32, pageIdx: UInt32, x: CGFloat, y: CGFloat) {
        guard let readerSession else { return }
        guard let hit = try? readerSession.hitTest(
            spineIdx: spineIdx,
            pageIdx: pageIdx,
            x: Double(x),
            y: Double(y)
        ), let target = hit.linkTarget else {
            showLinkNotFollowed()
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
        do { try attemptJump(resolveJump(target, linkToast: true)) }
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

    /// A programmatic turn met a chapter still laying out: park it as a
    /// jump and let that chapter's layout event complete it. A forward
    /// crossing needs only the target's first page, so it aims at offset
    /// 0; a backward crossing needs complete geometry to know the last
    /// page, which `toChapterEnd` waits for.
    func parkBoundaryTurn(direction: CGFloat) {
        guard let surface = pagerSurface else { return }
        let forward = (direction > 0) != surface.isRightToLeft
        let target = Int64(surface.spineIdx) + (forward ? 1 : -1)
        guard target >= 0, target < Int64(surface.spineCount) else { return }
        let jump = PendingJump(
            coordinate: Coordinate(spineIdx: UInt32(target), charOffset: forward ? 0 : .max),
            toChapterEnd: !forward,
            showChrome: false
        )
        do { try attemptJump(jump) }
        catch { logger.warning("Boundary turn to spine \(target, privacy: .public) failed: \(error)") }
    }

    func jump(to coordinate: Coordinate) throws {
        try attemptJump(PendingJump(coordinate: coordinate))
    }

    func attemptJump(_ pending: PendingJump) throws {
        guard let readerSession, let surface = pagerSurface else { return }
        selectionController?.clear()
        pager?.cancelInteraction()
        var jump = pending
        let spineIdx = jump.coordinate.spineIdx
        if let anchor = jump.anchor {
            // The anchor map arrives with the complete chapter. `NotReady`
            // means "not yet", so park and let the chapter's readiness event
            // retry; only `AnchorNotFound` (thrown on) means it is missing.
            do {
                jump = jump.resolved(
                    to: try readerSession.locateHref(href: anchor.href, fragment: anchor.fragment)
                )
            } catch InkunaError.NotReady {
                pendingJump = jump
                _ = try? readerSession.chapter(spineIdx: spineIdx)
                return
            }
        }
        if jump.toChapterEnd && !readerSession.isReady(spineIdx: spineIdx) {
            pendingJump = jump
            _ = try? readerSession.chapter(spineIdx: spineIdx)
            return
        }
        do {
            let wasReady = readerSession.isReady(spineIdx: spineIdx)
            let location = try readerSession.locate(coordinate: jump.coordinate)
            guard accept(generation: location.generation) else { return }
            surface.display(spineIdx: location.spineIdx, pageIdx: location.pageIdx)
            if let matchLength = jump.matchLength {
                let rects: [SelectionRect]
                do {
                    let pageRange = try readerSession.pageCharRange(
                        spineIdx: location.spineIdx,
                        pageIdx: location.pageIdx
                    )
                    let (matchEnd, overflow) = jump.coordinate.charOffset.addingReportingOverflow(matchLength)
                    let start = max(jump.coordinate.charOffset, pageRange.start)
                    let end = min(overflow ? .max : matchEnd, pageRange.end)
                    rects = start < end
                        ? (try readerSession.matchRects(
                            spineIdx: location.spineIdx,
                            charOffset: start,
                            len: end - start
                        ))
                        : []
                } catch {
                    rects = []
                }
                canvas?.showSearchHighlight(rects)
            }
            pendingJump = wasReady ? nil : jump
            if jump.showChrome { setChrome(visible: true) }
        } catch InkunaError.NotReady {
            pendingJump = jump
            _ = try? readerSession.chapter(spineIdx: spineIdx)
        }
    }

    func jump(to chapter: Chapter) {
        do { try attemptJump(resolveJump(chapter, linkToast: true)) }
        catch InkunaError.AnchorNotFound { showLinkNotFollowed() }
        catch { logger.warning("TOC jump for \(chapter.id, privacy: .public) failed: \(error)") }
    }

    func jump(to bookmark: Bookmark) {
        let coordinate: Coordinate
        if let stored = bookmark.coordinate {
            coordinate = stored
        } else if let readerSession,
                  let restored = coordinateForProgression(bookmark.progression, session: readerSession) {
            coordinate = restored
        } else {
            showLinkNotFollowed()
            return
        }
        do { try jump(to: coordinate) }
        catch { logger.warning("Bookmark jump failed: \(error)") }
    }

    func coordinateForProgression(_ progression: Double, session: ReaderSession) -> Coordinate? {
        let count = session.positionCount()
        guard count > 0 else { return nil }
        let position = min(max(UInt32((progression * Double(count)).rounded()), 1), count)
        return session.coordinateAtPosition(position: position)
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
                    guard let self else { return nil }
                    do {
                        let shelf = try await LibraryStore.shared.library()
                        return Int(try await ReaderPositions.position(
                            of: Coordinate(spineIdx: hit.spineIdx, charOffset: UInt64(hit.charOffset)),
                            id: self.publication.id,
                            on: shelf
                        ))
                    } catch {
                        self.logger.warning("Search position failed: \(error)")
                        return nil
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
            try attemptJump(PendingJump(
                coordinate: Coordinate(spineIdx: hit.spineIdx, charOffset: UInt64(hit.charOffset)),
                matchLength: UInt64(hit.snippetMatch.unicodeScalars.count)
            ))
            hideSearch()
        } catch {
            logger.warning("Search jump failed: \(error)")
        }
    }
}
