package app.inkuna.android.ui.reader

import android.util.Log
import android.view.ViewGroup
import android.view.accessibility.AccessibilityManager
import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.displayCutout
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowBack
import androidx.compose.material.icons.automirrored.outlined.List
import androidx.compose.material.icons.filled.Bookmark
import androidx.compose.material.icons.outlined.Bookmark
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material.icons.outlined.FormatSize
import androidx.compose.material.icons.outlined.LinkOff
import androidx.compose.material.icons.outlined.MoreHoriz
import androidx.compose.material.icons.outlined.Search
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.FrameRateCategory
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.preferredFrameRate
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.max
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.compose.LifecycleStartEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.inkuna.android.R
import app.inkuna.android.model.AppSettings
import app.inkuna.android.ui.components.InkButton
import app.inkuna.android.ui.components.InkButtonSize
import app.inkuna.android.ui.components.InkToast
import app.inkuna.android.ui.reader.engine.EnginePageCanvas
import app.inkuna.android.ui.reader.engine.EnginePagerSurface
import app.inkuna.android.ui.reader.engine.PagePalette
import app.inkuna.android.ui.reader.engine.ReaderFontStore
import app.inkuna.android.ui.reader.engine.ReaderSelectionController
import app.inkuna.android.ui.theme.InkMotion
import app.inkuna.android.ui.theme.InkType
import app.inkuna.android.ui.theme.ReadingFont
import app.inkuna.core.Chapter
import app.inkuna.core.Coordinate
import app.inkuna.core.InkunaException
import app.inkuna.core.PageLocation
import app.inkuna.core.Viewport
import kotlin.math.roundToInt
import kotlinx.coroutines.delay

@Composable
fun ReaderScreen(
    publicationId: String,
    settings: AppSettings,
    snapshot: AppSettings.Snapshot,
    onBack: () -> Unit,
    initialChapterHref: String? = null,
) {
    val viewModel: ReaderViewModel = viewModel(
        key = "reader-$publicationId",
        factory = ReaderViewModel.factory(publicationId, initialChapterHref),
    )
    val state by viewModel.state.collectAsStateWithLifecycle()
    val theme = snapshot.readingTheme
    val background by animateColorAsState(theme.background, tween(InkMotion.durMed, easing = InkMotion.easeQuiet), "readerBg")
    val foreground by animateColorAsState(theme.foreground, tween(InkMotion.durMed, easing = InkMotion.easeQuiet), "readerFg")
    val statusPad = WindowInsets.statusBars.asPaddingValues().calculateTopPadding()
    val navPad = WindowInsets.navigationBars.asPaddingValues().calculateBottomPadding()
    val tablet = LocalConfiguration.current.smallestScreenWidthDp >= 600
    val contentTop = ReaderMetrics.contentTop(
        max(statusPad, WindowInsets.displayCutout.asPaddingValues().calculateTopPadding()),
        tablet,
    )
    val contentBottom = ReaderMetrics.contentBottom(navPad, tablet)

    // The engine viewport is the reader surface's measured size, not any
    // WindowManager metric — the activity can be a split-screen or freeform
    // pane much smaller than the display.
    BoxWithConstraints(Modifier.fillMaxSize().background(background)) {
        val viewport = Viewport(
            width = maxWidth.value.toDouble(),
            height = (maxHeight - contentTop - contentBottom).value.toDouble().coerceAtLeast(0.0),
        )
        LaunchedEffect(viewport) { viewModel.open(viewport) }
        when (val current = state) {
            ReaderViewModel.UiState.Opening -> Unit
            ReaderViewModel.UiState.Failed -> ReaderOpenFailed(
                foreground,
                stringResource(R.string.reader_open_failed),
                { viewModel.open(viewport, userRetry = true) },
                Modifier.align(Alignment.Center),
            )
            ReaderViewModel.UiState.FixedLayoutUnsupported -> ReaderOpenFailed(
                foreground,
                stringResource(R.string.reader_fixed_layout_unsupported),
                null,
                Modifier.align(Alignment.Center),
            )
            ReaderViewModel.UiState.NoReadableContent -> ReaderOpenFailed(
                foreground,
                stringResource(R.string.reader_book_empty),
                null,
                Modifier.align(Alignment.Center),
            )
            is ReaderViewModel.UiState.Ready -> ReaderContent(
                viewModel, current.book, settings, snapshot, foreground, statusPad, navPad,
                contentTop, contentBottom, viewport, onBack,
            )
        }
        if (state !is ReaderViewModel.UiState.Ready) {
            Box(Modifier.align(Alignment.TopStart).padding(start = 16.dp, top = statusPad + 6.dp)) {
                ReaderGlassButton(Icons.AutoMirrored.Outlined.ArrowBack, stringResource(R.string.a11y_back), onBack)
            }
        }
    }
}

@Composable
private fun ReaderContent(
    viewModel: ReaderViewModel,
    book: ReaderViewModel.ReaderBook,
    settings: AppSettings,
    snapshot: AppSettings.Snapshot,
    foreground: Color,
    statusPad: Dp,
    navPad: Dp,
    contentTop: Dp,
    contentBottom: Dp,
    viewport: Viewport,
    onBack: () -> Unit,
) {
    val context = LocalContext.current
    val haptics = LocalHapticFeedback.current
    val chromeVisible = rememberSaveable { mutableStateOf(true) }
    val menuOpen = rememberSaveable { mutableStateOf(false) }
    var themeSheetOpen by rememberSaveable { mutableStateOf(false) }
    var contentsSheetOpen by rememberSaveable { mutableStateOf(false) }
    val searchOpen = rememberSaveable { mutableStateOf(false) }
    val anchorState = remember(book) { mutableStateOf<Coordinate?>(null) }
    val hostState = remember(book) { mutableStateOf<EngineHost?>(null) }
    // Generation staleness lives in the retained ViewModel (the sole pin
    // over the engine's generation); composition state here is only what
    // dies legitimately with the composition.
    val pendingJumpState = remember(book) { mutableStateOf<PendingJump?>(null) }
    var pendingJump by pendingJumpState
    val notedTruncatedChapters = remember(book) { mutableStateOf(setOf<UInt>()) }
    var brightnessPreview by remember { mutableStateOf<Float?>(null) }
    var toastCount by rememberSaveable { mutableIntStateOf(0) }
    var toastShown by rememberSaveable { mutableIntStateOf(0) }
    val toastVisible = remember { mutableStateOf(false) }
    val toastMessage = rememberSaveable { mutableStateOf(ReaderToast.BookmarkPlaced) }
    // Live TalkBack state: read inside the callback bodies below so toggling
    // touch exploration mid-session changes behavior without leaving the screen.
    val touchExploration = remember { mutableStateOf(false) }
    DisposableEffect(context) {
        val manager = context.getSystemService(AccessibilityManager::class.java)
        touchExploration.value = manager?.isTouchExplorationEnabled == true
        val listener = AccessibilityManager.TouchExplorationStateChangeListener { enabled ->
            touchExploration.value = enabled
        }
        manager?.addTouchExplorationStateChangeListener(listener)
        onDispose { manager?.removeTouchExplorationStateChangeListener(listener) }
    }

    fun notifyLinkFailed() {
        toastMessage.value = ReaderToast.LinkFailed
        toastCount += 1
    }

    fun display(
        location: PageLocation,
        host: EngineHost,
        showChrome: Boolean = true,
        searchMatch: PendingJump? = null,
    ) {
        host.selection.clear()
        host.layout.cancelInteraction()
        val chromeWas = chromeVisible.value
        host.surface.display(location.spineIdx, location.pageIdx)
        searchMatch?.matchLength?.let { length ->
            val rects = runCatching {
                val pageRange = book.session.pageCharRange(location.spineIdx, location.pageIdx)
                val matchEnd = if (ULong.MAX_VALUE - searchMatch.coordinate.charOffset < length) {
                    ULong.MAX_VALUE
                } else {
                    searchMatch.coordinate.charOffset + length
                }
                val start = maxOf(searchMatch.coordinate.charOffset, pageRange.start)
                val end = minOf(matchEnd, pageRange.end)
                if (start < end) {
                    book.session.matchRects(location.spineIdx, start, end - start)
                } else {
                    emptyList()
                }
            }.getOrDefault(emptyList())
            host.canvas.showSearchHighlight(rects)
        }
        anchorState.value = runCatching {
            Coordinate(location.spineIdx, book.session.pageCharRange(location.spineIdx, location.pageIdx).start)
        }.getOrNull()
        pendingJump = null
        chromeVisible.value = if (showChrome) true else chromeWas
    }

    fun attemptJump(pending: PendingJump, host: EngineHost) {
        host.selection.clear()
        host.layout.cancelInteraction()
        var jump = pending
        val spineIdx = jump.coordinate.spineIdx
        val anchor = jump.anchor
        if (anchor != null) {
            // The anchor map arrives with the complete chapter. `NotReady`
            // means "not yet", so park and let the chapter's readiness event
            // retry; only `AnchorNotFound` means the anchor is missing.
            try {
                jump = jump.copy(coordinate = book.session.locateHref(anchor.href, anchor.fragment), anchor = null)
            } catch (_: InkunaException.NotReady) {
                pendingJump = jump
                runCatching { book.session.chapter(spineIdx) }
                return
            } catch (_: InkunaException) {
                pendingJump = null
                if (jump.linkToast) notifyLinkFailed()
                return
            }
        }
        if (jump.toChapterEnd && !book.session.isReady(spineIdx)) {
            // A deferred backward turn lands on the last page, which only
            // complete geometry knows; `locate` would clamp to the
            // published prefix. Park it and schedule the chapter.
            pendingJump = jump
            runCatching { book.session.chapter(spineIdx) }
            return
        }
        try {
            // Read readiness BEFORE locating: a partially laid chapter
            // clamps `locate` to its published prefix, and readiness is
            // monotonic within a generation — so a pre-read decides
            // race-free whether the result can be a clamped page. Checking
            // after `locate` loses the jump when the chapter completes in
            // between: the clamped page shows, yet nothing re-presents.
            val wasReady = runCatching { book.session.isReady(spineIdx) }.getOrDefault(false)
            display(book.session.locate(jump.coordinate), host, jump.showChrome, jump)
            // Keep a possibly-clamped jump parked so chapter completion
            // re-presents it exactly (a user page turn supersedes it via
            // onPageSettled).
            if (!wasReady) pendingJump = jump
        } catch (_: InkunaException.NotReady) {
            pendingJump = jump
            runCatching { book.session.chapter(spineIdx) }
        } catch (_: InkunaException.AnchorNotFound) {
            pendingJump = null
            if (jump.linkToast) notifyLinkFailed()
        } catch (failure: InkunaException) {
            Log.w("InkunaReader", "jump to ${jump.coordinate} failed", failure)
            pendingJump = null
            if (jump.linkToast) notifyLinkFailed()
        }
    }

    fun presentPending(host: EngineHost, spineIdx: UInt) {
        // Only an event for the parked spine may retry — anything else
        // would re-jump to the current page on every background chapter.
        val jump = pendingJump ?: return
        if (jump.coordinate.spineIdx != spineIdx) return
        attemptJump(jump, host)
    }

    fun handleEvent(event: ReaderViewModel.LayoutEvent, host: EngineHost) {
        when (event) {
            ReaderViewModel.LayoutEvent.Invalidated -> {
                host.selection.clear()
                host.layout.cancelInteraction()
                pendingJump = viewModel.currentCoordinate()?.let { PendingJump(it, showChrome = false) }
                host.surface.layoutInvalidated(0uL)
            }
            is ReaderViewModel.LayoutEvent.FirstPage -> {
                host.surface.firstPageBecameReady(event.generation, event.spineIdx)
                viewModel.onFirstPageReady(event.spineIdx)
                presentPending(host, event.spineIdx)
            }
            is ReaderViewModel.LayoutEvent.Chapter -> {
                host.surface.chapterBecameReady(event.generation, event.spineIdx)
                if (event.spineIdx !in notedTruncatedChapters.value &&
                    runCatching { book.session.chapter(event.spineIdx).truncated }.getOrDefault(false)
                ) {
                    notedTruncatedChapters.value += event.spineIdx
                    host.canvas.showTruncationNotice()
                }
                viewModel.onChapterReady(event.spineIdx)
                presentPending(host, event.spineIdx)
            }
            is ReaderViewModel.LayoutEvent.Failed -> {
                host.surface.chapterFailed(event.generation, event.spineIdx)
                val parked = pendingJump
                val jumpSpine = parked?.coordinate?.spineIdx
                val targetSpine = viewModel.currentCoordinate()?.spineIdx
                if (jumpSpine == event.spineIdx) {
                    pendingJump = null
                    // Terminal for this jump: the chapter it waited on will
                    // never lay out, so say so once instead of parking forever.
                    if (parked?.linkToast == true) notifyLinkFailed()
                }
                if (jumpSpine == event.spineIdx || targetSpine == event.spineIdx || anchorState.value?.spineIdx == event.spineIdx) {
                    host.surface.display(event.spineIdx, 0u)
                }
            }
        }
    }

    val host = hostState.value
    if (host != null) {
        LaunchedEffect(book, host) {
            if (viewModel.consumeInitialHrefFailure()) notifyLinkFailed()
            // The canvas must be measured before anything is displayed:
            // a zero-width strip would anchor every page at slot 0 and pin
            // the pager unengageable (iOS guarantees this with
            // layoutIfNeeded before installing its canvas).
            host.canvas.awaitSized()
            viewModel.takeInitialLocation()?.let { display(it, host) }
                ?: run {
                    if (anchorState.value == null) {
                        // A rebuilt composition over the retained session:
                        // restore the current place without stealing focus
                        // from the saved chrome state.
                        viewModel.currentCoordinate()?.let {
                            attemptJump(PendingJump(it, showChrome = false), host)
                        }
                    }
                }
            // An open-time fragment link lands on the chapter start above and
            // refines to its anchor when that chapter finishes laying out.
            viewModel.takeInitialJump()?.let { attemptJump(it, host) }
            viewModel.layoutEvents.collect { event -> handleEvent(event, host) }
        }
    }

    fun requestRelayout() {
        val live = hostState.value ?: return
        live.selection.clear()
        live.layout.cancelInteraction()
        viewModel.requestAppearanceUpdate(viewModel.settingsFor(snapshot), viewport)
    }

    var appliedTypography by remember(book) { mutableStateOf(false) }
    LaunchedEffect(
        snapshot.textSizeStep, snapshot.rawReadingFont, snapshot.readingBold, snapshot.lineSpacing,
        snapshot.letterSpacing, snapshot.wordSpacing, snapshot.readingMargins,
    ) {
        if (appliedTypography) requestRelayout() else appliedTypography = true
    }
    LaunchedEffect(viewport) {
        // The session outlives the activity, so compare against the
        // viewport it actually laid out for — a composition-scoped flag
        // resets across rotation and would skip the relayout. Keying on the
        // measured viewport also catches split-screen divider drags that
        // resize the pane without recreating the activity.
        if (viewModel.needsViewportRelayout(viewport)) requestRelayout()
    }
    LaunchedEffect(toastCount) {
        if (toastCount > toastShown) {
            toastShown = toastCount
            toastVisible.value = true
            delay(1800)
            toastVisible.value = false
        }
    }
    // The faces are built off the main thread; a page that drew before they
    // landed is repainted here rather than staying blank.
    val fontRevision by ReaderFontStore.revision.collectAsStateWithLifecycle()
    LaunchedEffect(fontRevision, host) { if (fontRevision > 0) host?.canvas?.redrawPages() }
    LifecycleStartEffect(book) {
        viewModel.onReaderVisible()
        onStopOrDispose { viewModel.onReaderHidden() }
    }
    BackHandler(enabled = searchOpen.value || menuOpen.value) {
        if (searchOpen.value) {
            searchOpen.value = false
            chromeVisible.value = true
        } else menuOpen.value = false
    }

    val keysSuppressed = themeSheetOpen || contentsSheetOpen || searchOpen.value
    Box(Modifier.fillMaxSize().readerKeyTurns(host, keysSuppressed)) {
        AndroidView(
            factory = { viewContext ->
                val canvas = EnginePageCanvas(viewContext).apply { palette = PagePalette.from(snapshot.readingTheme) }
                val surface = EnginePagerSurface(book.session, canvas).apply { spineCount = book.spineCount }
                val selection = ReaderSelectionController(book.session, canvas, surface)
                val layout = ReaderPagerLayout(viewContext).apply {
                    addView(canvas, ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
                    bind(surface)
                }
                val engineHost = EngineHost(layout, canvas, surface, selection)
                surface.onPageSettled = { spineIdx, pageIdx ->
                    selection.clear()
                    // A settled page supersedes any parked navigation; a
                    // presenting jump re-parks itself right after this.
                    pendingJumpState.value = null
                    viewModel.onPageSettled(spineIdx, pageIdx)
                    anchorState.value = runCatching {
                        Coordinate(spineIdx, book.session.pageCharRange(spineIdx, pageIdx).start)
                    }.getOrNull()
                    if (!touchExploration.value) {
                        chromeVisible.value = false
                        menuOpen.value = false
                    }
                }
                canvas.onPageDrawn = { spineIdx, pageIdx ->
                    if (surface.spineIdx == spineIdx && surface.pageIdx == pageIdx) viewModel.onCurrentPageDrawn()
                }
                canvas.onLinkActivated = { spineIdx, pageIdx, x, y ->
                    handleLinkActivation(book, engineHost, spineIdx, pageIdx, x, y, ::notifyLinkFailed) { jump ->
                        attemptJump(jump, engineHost)
                    }
                }
                canvas.onPageTap = { spineIdx, pageIdx, x, y ->
                    handleCanvasPoint(book, engineHost, x, y, ::notifyLinkFailed, { jump -> attemptJump(jump, engineHost) }, chromeVisible, menuOpen)
                }
                layout.onTurnGesture = {
                    if (!touchExploration.value) {
                        chromeVisible.value = false
                        menuOpen.value = false
                    }
                }
                layout.onBoundaryTurnPending = { sign ->
                    // A programmatic turn met a chapter still laying out:
                    // park it as a jump and finish it on that chapter's
                    // layout event (backward waits for complete geometry).
                    val forward = (sign > 0) != surface.isRightToLeft
                    val target = surface.spineIdx.toLong() + if (forward) 1 else -1
                    if (target in 0 until book.spineCount.toLong()) {
                        attemptJump(
                            PendingJump(
                                coordinate = Coordinate(target.toUInt(), if (forward) 0uL else ULong.MAX_VALUE),
                                toChapterEnd = !forward,
                                showChrome = false,
                            ),
                            engineHost,
                        )
                    }
                }
                hostState.value = engineHost
                layout
            },
            update = { live -> hostState.value?.canvas?.palette = PagePalette.from(snapshot.readingTheme) },
            modifier = Modifier.fillMaxSize().padding(top = contentTop, bottom = contentBottom).semantics {
                onClick(label = context.getString(R.string.a11y_toggle_reader_controls)) {
                    chromeVisible.value = !chromeVisible.value
                    true
                }
                customActions = listOf(
                    CustomAccessibilityAction(context.getString(R.string.a11y_next_page)) { hostState.value?.layout?.turnForward() ?: false },
                    CustomAccessibilityAction(context.getString(R.string.a11y_previous_page)) { hostState.value?.layout?.turnBackward() ?: false },
                )
            },
        )

        val brightness = brightnessPreview ?: snapshot.brightness
        val veil = (AppSettings.DEFAULT_BRIGHTNESS - brightness).coerceAtLeast(0f) / 1.7f
        if (veil > 0f) Box(Modifier.fillMaxSize().drawBehind { drawRect(Color(0xFF0A0907).copy(alpha = veil)) })

        val placeBookmark = {
            val coordinate = anchorState.value ?: viewModel.currentCoordinate()
            if (coordinate == null) {
                toastMessage.value = ReaderToast.LinkFailed
                toastCount += 1
            } else {
                val count = book.session.positionCount().coerceAtLeast(1u)
                val progression = book.session.positionOf(coordinate).toDouble() / count.toDouble()
                viewModel.addBookmark(coordinate, progression) {
                    haptics.performHapticFeedback(androidx.compose.ui.hapticfeedback.HapticFeedbackType.Confirm)
                    toastMessage.value = ReaderToast.BookmarkPlaced
                    toastCount += 1
                }
            }
        }
        ReaderChromeLayer(
            book, foreground, statusPad, navPad, contentBottom, chromeVisible, menuOpen, searchOpen,
            toastVisible, toastMessage, anchorState, { hostState.value?.selection?.clear(); onBack() },
            onOpenContents = { contentsSheetOpen = true }, onOpenThemeType = { themeSheetOpen = true },
            onPlaceBookmark = placeBookmark,
            onSelectSearch = { hit ->
                hit.charOffset?.let { offset ->
                    hostState.value?.let {
                        attemptJump(PendingJump(Coordinate(hit.spineIdx, offset), matchLength = hit.matchLength), it)
                    }
                }
                searchOpen.value = false
                chromeVisible.value = true
            },
            onCloseSearch = { searchOpen.value = false; chromeVisible.value = true },
            viewModel = viewModel,
        )
    }

    if (themeSheetOpen) {
        // The Publisher roster entry previews in the face the page is
        // actually read in: the dominant glyph-run face of the current
        // page, resolved through the primed font store. Re-sampled on
        // layout events and font-store builds, so a fresh Publisher pick
        // settles onto the embedded face once the reflow lands.
        var layoutTick by remember(book) { mutableIntStateOf(0) }
        LaunchedEffect(book) { viewModel.layoutEvents.collect { layoutTick += 1 } }
        val publisherFamily = if (snapshot.readingFont == ReadingFont.Publisher) {
            remember(book, layoutTick, fontRevision, anchorState.value) {
                publisherReadingFamily(book.session, hostState.value)
            }
        } else {
            null
        }
        ThemeTypeSheet(
            snapshot,
            settings,
            publisherFamily = publisherFamily,
            onBrightnessPreview = { brightnessPreview = it },
        ) { themeSheetOpen = false }
    }
    if (contentsSheetOpen) {
        // One position/count snapshot feeds both the highlight row and the
        // header line — two FFI crossings for the whole sheet, not three.
        val (sheetPosition, sheetCount) = readerPositionSnapshot(book, anchorState.value)
        ContentsSheet(
            publication = book.publication,
            chapters = book.chapters,
            positionRanges = book.positionRanges,
            currentPosition = sheetPosition,
            pageInfo = readerPageInfo(book, sheetPosition, sheetCount),
            onSelect = { chapter ->
                hostState.value?.let { live ->
                    runCatching { book.session.resolveJump(chapter.href, linkToast = true) }
                        .onSuccess { attemptJump(it, live) }
                        .onFailure { notifyLinkFailed() }
                }
            },
            onDismiss = { contentsSheetOpen = false },
        )
    }
}
