import UIKit

extension ReaderViewController {
    func openReader() async {
        view.layoutIfNeeded()
        let relay = ReaderLayoutRelay(
            onFirstPageReady: { [weak self] generation, spineIdx in
                Task { @MainActor in self?.receive(.first(generation, spineIdx)) }
            },
            onChapterReady: { [weak self] generation, spineIdx, count in
                Task { @MainActor in self?.receive(.complete(generation, spineIdx, count)) }
            },
            onChapterFailed: { [weak self] generation, spineIdx in
                Task { @MainActor in self?.receive(.failed(generation, spineIdx)) }
            }
        )
        do {
            openedAt = Date()
            let shelf = try await LibraryStore.shared.library()
            let reader = try await shelf.openReader(
                id: publication.id,
                viewport: viewport(),
                settings: layoutSettings(),
                listener: relay
            )
            guard !Task.isCancelled, isViewLoaded else { return }
            readerSession = reader
            layoutRelay = relay
            ReaderFontStore.shared.prime(reader.fontRegistry())
            installCanvas(session: reader)
            if let initialChapter {
                targetCoordinate = try resolveHref(initialChapter)
            } else {
                targetCoordinate = publication.coordinate ?? Coordinate(spineIdx: 0, charOffset: 0)
            }
            if let targetCoordinate {
                _ = try? reader.page(spineIdx: targetCoordinate.spineIdx, pageIdx: 0)
            }
            tryPresentTarget()
            fetchChapters()
            let events = pendingEvents
            pendingEvents.removeAll()
            events.forEach(process)
        } catch is CancellationError {
            return
        } catch {
            logger.error("Opening \(self.publication.id, privacy: .public) failed: \(error)")
            showOpenFailure()
        }
    }

    func installCanvas(session: ReaderSession) {
        let canvas = EnginePageCanvas(session: session, theme: AppSettings.shared.readingTheme)
        canvas.translatesAutoresizingMaskIntoConstraints = false
        canvas.onTap = { [weak self] in self?.handleCanvasTap($0) }
        canvas.onPageDrawn = { [weak self] spineIdx, pageIdx in
            self?.pageDidDraw(spineIdx: spineIdx, pageIdx: pageIdx)
        }
        view.insertSubview(canvas, at: 0)
        let top = canvas.topAnchor.constraint(equalTo: view.topAnchor)
        let bottom = canvas.bottomAnchor.constraint(equalTo: view.bottomAnchor)
        canvasTop = top
        canvasBottom = bottom
        NSLayoutConstraint.activate([
            canvas.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            canvas.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            top, bottom,
        ])
        self.canvas = canvas
        updateReadingBand()
        view.layoutIfNeeded()

        let surface = EnginePagerSurface(session: session, canvas: canvas)
        surface.spineCount = session.spineCount()
        surface.onPageSettled = { [weak self] spineIdx, pageIdx in
            self?.selectionController?.clear()
            self?.pageSettled(spineIdx: spineIdx, pageIdx: pageIdx)
        }
        pagerSurface = surface
        selectionController = ReaderSelectionController(
            session: session,
            canvas: canvas,
            surface: surface,
            presenter: self
        )
        let pager = ReaderPager(surface: surface, view: canvas)
        pager.onPageTurnGesture = { [weak self] in
            guard !UIAccessibility.isVoiceOverRunning else { return }
            self?.setChrome(visible: false)
        }
        self.pager = pager
    }

    func receive(_ event: LayoutEvent) {
        if layoutChangeInFlight, event.generation == generationBeforeLayout { return }
        guard pagerSurface != nil, !layoutChangeInFlight else {
            pendingEvents.append(event)
            return
        }
        process(event)
    }

    func process(_ event: LayoutEvent) {
        switch event {
        case let .first(generation, spineIdx):
            guard accept(generation: generation) else { return }
            pagerSurface?.firstPageBecameReady(generation: generation, spineIdx: spineIdx)
            if spineIdx == targetCoordinate?.spineIdx {
                firstPageReadyAt = firstPageReadyAt ?? Date()
                log("open_to_first_page_ready_ms", since: openedAt)
                tryPresentTarget()
            }
        case let .complete(generation, spineIdx, _):
            guard accept(generation: generation) else { return }
            pagerSurface?.chapterBecameReady(generation: generation, spineIdx: spineIdx)
            if spineIdx == targetCoordinate?.spineIdx, !didLogChapterComplete {
                didLogChapterComplete = true
                log("chapter_layout_complete_ms", since: openedAt)
            }
        case let .failed(generation, spineIdx):
            guard accept(generation: generation) else { return }
            pagerSurface?.chapterFailed(generation: generation, spineIdx: spineIdx)
            if spineIdx == targetCoordinate?.spineIdx {
                pagerSurface?.display(spineIdx: spineIdx, pageIdx: 0)
                didPresentInitialPage = true
                loadingIndicator.stopAnimating()
            }
        }
    }

    func accept(generation: UInt64) -> Bool {
        if let layoutGeneration, generation != layoutGeneration { return false }
        if layoutGeneration == nil, generation == generationBeforeLayout { return false }
        layoutGeneration = generation
        return true
    }

    func tryPresentTarget() {
        guard let readerSession, let pagerSurface, let targetCoordinate,
              let location = try? readerSession.locate(coordinate: targetCoordinate),
              accept(generation: location.generation) else { return }
        pagerSurface.display(spineIdx: location.spineIdx, pageIdx: location.pageIdx)
        didPresentInitialPage = true
        loadingIndicator.stopAnimating()
        firstPageReadyAt = firstPageReadyAt ?? Date()
        relayoutAnchor = nil
    }

    func resolveHref(_ chapter: Chapter) throws -> Coordinate { try resolveHref(chapter.href) }

    func resolveHref(_ href: String) throws -> Coordinate {
        guard let readerSession else { throw InkunaError.NotReady(detail: "Reader is not open") }
        guard let hashIndex = href.firstIndex(of: "#") else {
            return try readerSession.locateHref(href: href, fragment: nil)
        }
        return try readerSession.locateHref(
            href: String(href[..<hashIndex]),
            fragment: String(href[href.index(after: hashIndex)...])
        )
    }

    func viewport() -> Viewport {
        let inset = ReaderMetrics.contentInsets(
            safeArea: view.safeAreaInsets,
            isPad: traitCollection.userInterfaceIdiom == .pad
        )
        return Viewport(
            width: Double(view.bounds.width),
            height: Double(max(0, view.bounds.height - inset.top - inset.bottom))
        )
    }

    func layoutSettings() -> ReaderLayoutSettings {
        let settings = AppSettings.shared
        return ReaderLayoutSettings(
            readingFont: ReadingFont.normalize(settings.readingFontID).rawValue,
            readingBold: settings.readingBold,
            textSizeStep: UInt8(settings.textSize.rawValue),
            lineSpacing: settings.lineSpacing,
            letterSpacing: settings.letterSpacing,
            wordSpacing: settings.wordSpacing,
            readingMargins: UInt32(settings.readingMargins)
        )
    }

    func updateReadingBand() {
        let inset = ReaderMetrics.contentInsets(
            safeArea: view.safeAreaInsets,
            isPad: traitCollection.userInterfaceIdiom == .pad
        )
        canvasTop?.constant = inset.top
        canvasBottom?.constant = -inset.bottom
    }

    func fetchChapters() {
        Task { [weak self, id = publication.id, logger] in
            do {
                let shelf = try await LibraryStore.shared.library()
                async let fetchedChapters = shelf.library().chapters(id: id)
                async let fetchedRanges = shelf.progress().chapterPositionRanges(id: id)
                let result = try await (fetchedChapters, fetchedRanges)
                guard let self, !Task.isCancelled else { return }
                self.chapters = result.0
                self.chapterRanges = result.1
            } catch {
                logger.warning("Fetching reader TOC for \(id, privacy: .public) failed: \(error)")
            }
        }
    }

    func currentAnchor() -> Coordinate? {
        guard let readerSession, let surface = pagerSurface,
              let range = try? readerSession.pageCharRange(spineIdx: surface.spineIdx, pageIdx: surface.pageIdx) else { return nil }
        return Coordinate(spineIdx: surface.spineIdx, charOffset: range.start)
    }

    func pageSettled(spineIdx: UInt32, pageIdx: UInt32) {
        guard let readerSession,
              let range = try? readerSession.pageCharRange(spineIdx: spineIdx, pageIdx: pageIdx) else { return }
        let coordinate = Coordinate(spineIdx: spineIdx, charOffset: range.start)
        updatePageInfo()
        if announcePageWhenSettled {
            announcePageWhenSettled = false
            UIAccessibility.post(notification: .pageScrolled, argument: pageInfoText())
        }
        enqueueCoreWrite("progress") { [id = publication.id] shelf in
            let position = try await ReaderPositions.position(of: coordinate, id: id, on: shelf)
            let progression = try await ReaderPositions.progression(of: coordinate, id: id, on: shelf)
            try await shelf.progress().updateProgress(id: id, coordinate: coordinate, progression: progression, position: position)
        }
    }

    func pageDidDraw(spineIdx: UInt32, pageIdx: UInt32) {
        guard let surface = pagerSurface, surface.spineIdx == spineIdx, surface.pageIdx == pageIdx, !didLogFirstRender else { return }
        didLogFirstRender = true
        log("first_page_ready_to_first_render_ms", since: firstPageReadyAt)
        log("tap_to_first_page_ms", since: ReaderLauncher.lastPushTimestamp)
    }

    func log(_ name: String, since date: Date?) {
        guard let date else { return }
        perfLogger.info("\(name, privacy: .public) \(Int(Date().timeIntervalSince(date) * 1_000), privacy: .public)")
    }

    func relayout(anchor: Coordinate?) async {
        guard let readerSession else { return }
        selectionController?.clear()
        pager?.cancelInteraction()
        relayoutAnchor = anchor ?? currentAnchor()
        generationBeforeLayout = layoutGeneration
        layoutChangeInFlight = true
        do {
            try await readerSession.updateLayout(viewport: viewport(), settings: layoutSettings())
            layoutGeneration = nil
            targetCoordinate = relayoutAnchor ?? targetCoordinate
            pagerSurface?.layoutInvalidated(generation: 0)
            layoutChangeInFlight = false
            let events = pendingEvents
            pendingEvents.removeAll()
            events.forEach(process)
        } catch {
            layoutChangeInFlight = false
            logger.warning("Reader relayout failed: \(error)")
        }
    }

    func showOpenFailure() {
        loadingIndicator.stopAnimating()
        openFailureLabel.isHidden = false
    }

    func startSession() {
        enqueueCoreWrite("session start") { [session, id = publication.id] shelf in
            guard session.id == nil else { return }
            session.id = try await shelf.stats().sessionStart(id: id)
        }
    }

    func endSession() {
        enqueueCoreWrite("session end") { [session] shelf in
            guard let sessionID = session.id else { return }
            session.id = nil
            try await shelf.stats().sessionEnd(sessionId: sessionID)
        }
    }

    func enqueueCoreWrite(_ label: String, _ work: @escaping @MainActor (Bookshelf) async throws -> Void) {
        let previous = coreWriteChain
        coreWriteChain = Task { @MainActor [logger] in
            await previous?.value
            do {
                let shelf = try await LibraryStore.shared.library()
                try await work(shelf)
            } catch {
                logger.warning("Core write failed (\(label, privacy: .public)): \(error)")
            }
        }
    }

    override func viewWillTransition(to size: CGSize, with coordinator: any UIViewControllerTransitionCoordinator) {
        super.viewWillTransition(to: size, with: coordinator)
        let anchor = currentAnchor()
        coordinator.animate(alongsideTransition: nil) { [weak self] _ in
            Task { @MainActor in await self?.relayout(anchor: anchor) }
        }
    }
}
