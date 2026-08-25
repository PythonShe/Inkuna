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
            guard !Task.isCancelled, isViewLoaded else {
                // The abandoned session was never stored, so
                // `releaseReaderEngineOffMain()` cannot reach it. Dropping the
                // last reference here would run Rust `Drop` on the main actor,
                // where it joins the layout worker (up to one chapter's layout).
                // Hand it to a detached task instead, matching that path.
                Task.detached(priority: .utility) {
                    reader.shutdown()
                }
                return
            }
            readerSession = reader
            layoutRelay = relay
            // A publication whose spine holds no usable resource can never
            // emit a layout callback — the worker queue starts empty. The
            // count is known synchronously at open, so state it now rather
            // than waiting on an event that will not come.
            guard reader.spineCount() > 0 else {
                showOpenFailure(
                    String(
                        localized: "reader_book_empty",
                        defaultValue: "This book has no readable content."
                    )
                )
                return
            }
            // Awaited: the descriptor build runs off the main actor, but no
            // canvas exists — so no page draws — until priming completes.
            await ReaderFontStore.shared.prime(reader.fontRegistry(), owner: reader)
            // The await can outlive the screen: a pop during priming has
            // already shut the session down via `releaseReaderEngineOffMain`.
            guard !Task.isCancelled, readerSession === reader else { return }
            installCanvas(session: reader)
            let restoredCoordinate = publication.coordinate
                ?? coordinateForProgression(publication.progression, session: reader)
            if let initialChapter {
                do {
                    let jump = try resolveJump(initialChapter, linkToast: true)
                    targetCoordinate = jump.coordinate
                    if jump.anchor != nil { pendingJump = jump }
                } catch InkunaError.AnchorNotFound, InkunaError.NotReady {
                    targetCoordinate = restoredCoordinate
                    showLinkNotFollowed()
                }
            } else {
                targetCoordinate = restoredCoordinate
                if targetCoordinate == nil { showLinkNotFollowed() }
            }
            if let targetCoordinate {
                _ = try? reader.page(spineIdx: targetCoordinate.spineIdx, pageIdx: 0)
            }
            restorePending = targetCoordinate != nil
            tryPresentTarget()
            fetchChapters()
            let events = pendingEvents
            pendingEvents.removeAll()
            events.forEach(process)
        } catch is CancellationError {
            return
        } catch InkunaError.UnsupportedContent {
            showOpenFailure(
                String(
                    localized: "reader_fixed_layout_unsupported",
                    defaultValue: "This book uses a fixed layout, which Inkuna can't display yet."
                )
            )
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
        canvas.onLinkActivated = { [weak self] spineIdx, pageIdx, x, y in
            self?.activateLink(spineIdx: spineIdx, pageIdx: pageIdx, x: x, y: y)
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
            self?.pendingJump = nil
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
        pager.onBoundaryTurnPending = { [weak self] direction in
            self?.parkBoundaryTurn(direction: direction)
        }
        pager.yieldToSystemBackGesture(navigationController?.interactivePopGestureRecognizer)
        self.pager = pager
    }

    func receive(_ event: LayoutEvent) {
        guard pagerSurface != nil, !layoutChangeInFlight else {
            pendingEvents.append(event)
            return
        }
        guard accept(generation: event.generation) else { return }
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
            presentPendingJump(for: spineIdx)
        case let .complete(generation, spineIdx, _):
            guard accept(generation: generation) else { return }
            pagerSurface?.chapterBecameReady(generation: generation, spineIdx: spineIdx)
            showTruncationNoticeIfNeeded(for: spineIdx)
            if spineIdx == targetCoordinate?.spineIdx {
                tryPresentTarget()
            }
            if spineIdx == targetCoordinate?.spineIdx, !didLogChapterComplete {
                didLogChapterComplete = true
                log("chapter_layout_complete_ms", since: openedAt)
            }
            presentPendingJump(for: spineIdx)
        case let .failed(generation, spineIdx):
            guard accept(generation: generation) else { return }
            pagerSurface?.chapterFailed(generation: generation, spineIdx: spineIdx)
            let pendingJumpTargetsSpine = pendingJump?.coordinate.spineIdx == spineIdx
            if spineIdx == targetCoordinate?.spineIdx || pendingJumpTargetsSpine {
                pager?.cancelInteraction()
                pagerSurface?.display(spineIdx: spineIdx, pageIdx: 0)
                loadingIndicator.stopAnimating()
                // Terminal for the restore too: the chapter it anchored in
                // will never lay out, so a pinned anchor would shadow the
                // reader's real position forever.
                if relayoutAnchor?.spineIdx == spineIdx { relayoutAnchor = nil }
            }
            if pendingJumpTargetsSpine {
                let parked = pendingJump
                pendingJump = nil
                // Terminal for this jump: the chapter it waited on will
                // never lay out, so say so once instead of parking forever.
                if parked?.linkToast == true { showLinkNotFollowed() }
            }
        }
    }

    func presentPendingJump(for spineIdx: UInt32) {
        guard let jump = pendingJump, jump.coordinate.spineIdx == spineIdx else { return }
        do {
            try attemptJump(jump)
        } catch {
            logger.warning("Jump to spine \(jump.coordinate.spineIdx, privacy: .public) offset \(jump.coordinate.charOffset, privacy: .public) failed: \(error)")
            pendingJump = nil
            if jump.linkToast { showLinkNotFollowed() }
        }
    }

    func showTruncationNoticeIfNeeded(for spineIdx: UInt32) {
        guard !notedTruncatedChapters.contains(spineIdx),
              let readerSession,
              let geometry = try? readerSession.chapter(spineIdx: spineIdx),
              geometry.truncated else { return }
        notedTruncatedChapters.insert(spineIdx)
        canvas?.showTruncationNotice()
    }

    func accept(generation: UInt64) -> Bool {
        readerSession?.generation() == generation
    }

    func tryPresentTarget() {
        // A spent restore must stay spent: the target's chapter re-lays
        // whenever cache eviction cycles it out and back (routine while
        // crossing chapter boundaries near it), and its readiness events
        // land here again. Without this gate they would teleport the
        // reader back to the restore coordinate mid-read.
        guard restorePending else { return }
        guard let readerSession, let pagerSurface, let targetCoordinate,
              let location = try? readerSession.locate(coordinate: targetCoordinate),
              accept(generation: location.generation) else { return }
        // The presentation is a hard re-position; a page turn still in
        // flight would keep writing strip offsets over it.
        pager?.cancelInteraction()
        // The restore's own settle must not clear `relayoutAnchor`: the
        // anchor stays the restore coordinate itself — clamped or exact —
        // until the reader actually turns a page. Re-deriving it from the
        // landed page's start would lose up to a page per layout round
        // trip, ratcheting repeated appearance toggles back to the chapter
        // start.
        presentingRestore = true
        pagerSurface.display(spineIdx: location.spineIdx, pageIdx: location.pageIdx)
        presentingRestore = false
        loadingIndicator.stopAnimating()
        firstPageReadyAt = firstPageReadyAt ?? Date()
    }

    func resolveJump(_ chapter: Chapter, linkToast: Bool = false) throws -> PendingJump {
        try resolveJump(chapter.href, linkToast: linkToast)
    }

    /// Turns a TOC or link href into a jump. A fragment needs the target
    /// chapter's anchor map, which layout builds — so an un-laid chapter
    /// answers `NotReady`, not `AnchorNotFound`. The fragment-free lookup
    /// resolves from the spine model alone and never waits, so the jump
    /// still names the chapter, aims at its start, and carries the fragment
    /// for that chapter's readiness event to refine.
    func resolveJump(_ href: String, linkToast: Bool = false) throws -> PendingJump {
        guard let readerSession else { throw InkunaError.NotReady(detail: "Reader is not open") }
        guard let hashIndex = href.firstIndex(of: "#") else {
            return PendingJump(
                coordinate: try readerSession.locateHref(href: href, fragment: nil),
                linkToast: linkToast
            )
        }
        let path = String(href[..<hashIndex])
        let fragment = String(href[href.index(after: hashIndex)...])
        let chapterStart = try readerSession.locateHref(href: path, fragment: nil)
        do {
            return PendingJump(
                coordinate: try readerSession.locateHref(href: path, fragment: fragment),
                linkToast: linkToast
            )
        } catch InkunaError.NotReady {
            return PendingJump(
                coordinate: chapterStart,
                anchor: PendingAnchor(href: path, fragment: fragment),
                linkToast: linkToast
            )
        }
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
        // A pending relayout restore IS the logical position: the surface may
        // be showing a page clamped to the chapter's published prefix, and a
        // live read of it would leak the clamp into the next relayout's
        // anchor. `relayoutAnchor` clears once the restore presents exactly.
        if let relayoutAnchor { return relayoutAnchor }
        guard let readerSession, let surface = pagerSurface,
              let range = try? readerSession.pageCharRange(spineIdx: surface.spineIdx, pageIdx: surface.pageIdx) else { return nil }
        return Coordinate(spineIdx: surface.spineIdx, charOffset: range.start)
    }

    func pageSettled(spineIdx: UInt32, pageIdx: UInt32) {
        // A settle the reader caused — a page turn, a jump — supersedes any
        // pinned relayout restore; a restore's own presentation does not.
        if !presentingRestore {
            relayoutAnchor = nil
            restorePending = false
        }
        guard let readerSession,
              let range = try? readerSession.pageCharRange(spineIdx: spineIdx, pageIdx: pageIdx) else { return }
        let coordinate = Coordinate(spineIdx: spineIdx, charOffset: range.start)
        // Track the settled place so a relayout with no derivable anchor
        // falls back here, never to the long-superseded open coordinate.
        if !presentingRestore { targetCoordinate = coordinate }
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
        guard let surface = pagerSurface, surface.spineIdx == spineIdx, surface.pageIdx == pageIdx else { return }
        // A freshly drawn current page can carry a new layout: let an
        // open Customize preview re-sample the publisher face.
        customizePanel?.refreshPreview()
        guard !didLogFirstRender else { return }
        didLogFirstRender = true
        log("first_page_ready_to_first_render_ms", since: firstPageReadyAt)
        log("tap_to_first_page_ms", since: ReaderLauncher.lastPushTimestamp)
    }

    func log(_ name: String, since date: Date?) {
        guard let date else { return }
        perfLogger.info("\(name, privacy: .public) \(Int(Date().timeIntervalSince(date) * 1_000), privacy: .public)")
    }

    func relayout(anchor: Coordinate?) async {
        let previous = relayoutTask
        let next = Task { @MainActor [weak self] in
            await previous?.value
            guard let self, !Task.isCancelled else { return }
            await self.performRelayout(anchor: anchor)
        }
        relayoutTask = next
        await next.value
    }

    private func performRelayout(anchor: Coordinate?) async {
        guard let readerSession else { return }
        selectionController?.clear()
        pager?.cancelInteraction()
        relayoutAnchor = anchor ?? currentAnchor()
        layoutChangeInFlight = true
        do {
            try await readerSession.updateLayout(viewport: viewport(), settings: layoutSettings())
            targetCoordinate = relayoutAnchor ?? targetCoordinate
            restorePending = targetCoordinate != nil
            pagerSurface?.layoutInvalidated(generation: 0)
            layoutChangeInFlight = false
            let events = pendingEvents
            pendingEvents.removeAll()
            events.forEach(process)
        } catch {
            layoutChangeInFlight = false
            logger.warning("Reader relayout failed: \(error)")
            let events = pendingEvents
            pendingEvents.removeAll()
            events.forEach(process)
        }
    }

    func showOpenFailure(_ message: String? = nil) {
        loadingIndicator.stopAnimating()
        if let message { openFailureLabel.text = message }
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
