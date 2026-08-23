package app.inkuna.android.ui.reader

import android.content.Intent
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
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.State
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
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
import androidx.core.net.toUri
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
import app.inkuna.core.Chapter
import app.inkuna.core.Coordinate
import app.inkuna.core.InkunaException
import app.inkuna.core.PageLocation
import kotlin.math.roundToInt
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

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

    Box(Modifier.fillMaxSize().background(background)) {
        when (val current = state) {
            ReaderViewModel.UiState.Opening -> Unit
            ReaderViewModel.UiState.Failed -> ReaderOpenFailed(foreground, viewModel::open, Modifier.align(Alignment.Center))
            ReaderViewModel.UiState.FixedLayoutUnsupported -> ReaderOpenFailed(
                foreground,
                onBack,
                Modifier.align(Alignment.Center),
            )
            is ReaderViewModel.UiState.Ready -> ReaderContent(
                viewModel, current.book, settings, snapshot, foreground, statusPad, navPad, onBack,
            )
        }
        if (state !is ReaderViewModel.UiState.Ready) {
            Box(Modifier.align(Alignment.TopStart).padding(start = 16.dp, top = statusPad + 6.dp)) {
                ReaderGlassButton(Icons.AutoMirrored.Outlined.ArrowBack, stringResource(R.string.a11y_back), onBack)
            }
        }
    }
}

private class EngineHost(
    val layout: ReaderPagerLayout,
    val canvas: EnginePageCanvas,
    val surface: EnginePagerSurface,
    val selection: ReaderSelectionController,
)

@Composable
private fun ReaderContent(
    viewModel: ReaderViewModel,
    book: ReaderViewModel.ReaderBook,
    settings: AppSettings,
    snapshot: AppSettings.Snapshot,
    foreground: Color,
    statusPad: Dp,
    navPad: Dp,
    onBack: () -> Unit,
) {
    val context = LocalContext.current
    val haptics = LocalHapticFeedback.current
    val scope = rememberCoroutineScope()
    val chromeVisible = rememberSaveable { mutableStateOf(true) }
    val menuOpen = rememberSaveable { mutableStateOf(false) }
    var themeSheetOpen by rememberSaveable { mutableStateOf(false) }
    var contentsSheetOpen by rememberSaveable { mutableStateOf(false) }
    val searchOpen = rememberSaveable { mutableStateOf(false) }
    val anchorState = remember(book) { mutableStateOf<Coordinate?>(null) }
    val hostState = remember(book) { mutableStateOf<EngineHost?>(null) }
    val pendingEvents = remember(book) { mutableListOf<ReaderViewModel.LayoutEvent>() }
    var relayoutInFlight by remember(book) { mutableStateOf(false) }
    var discardGeneration by remember(book) { mutableStateOf<ULong?>(null) }
    var latestGeneration by remember(book) { mutableStateOf<ULong?>(null) }
    var pendingJump by remember(book) { mutableStateOf<Coordinate?>(null) }
    var brightnessPreview by remember { mutableStateOf<Float?>(null) }
    var toastCount by rememberSaveable { mutableIntStateOf(0) }
    var toastShown by rememberSaveable { mutableIntStateOf(0) }
    val toastVisible = remember { mutableStateOf(false) }
    val toastMessage = rememberSaveable { mutableStateOf(ReaderToast.BookmarkPlaced) }
    val configuration = LocalConfiguration.current
    val contentTop = ReaderMetrics.contentTop(
        max(statusPad, WindowInsets.displayCutout.asPaddingValues().calculateTopPadding()),
        configuration.smallestScreenWidthDp >= 600,
    )
    val contentBottom = ReaderMetrics.contentBottom(navPad, configuration.smallestScreenWidthDp >= 600)
    val touchExploration = remember(context) {
        context.getSystemService(AccessibilityManager::class.java)?.isTouchExplorationEnabled == true
    }

    fun notifyLinkFailed() {
        toastMessage.value = ReaderToast.LinkFailed
        toastCount += 1
    }

    fun display(location: PageLocation, host: EngineHost) {
        host.selection.clear()
        host.layout.cancelInteraction()
        host.surface.display(location.spineIdx, location.pageIdx)
        anchorState.value = Coordinate(location.spineIdx, book.session.pageCharRange(location.spineIdx, location.pageIdx).start)
        pendingJump = null
        chromeVisible.value = true
    }

    fun attemptJump(coordinate: Coordinate, host: EngineHost, linkToast: Boolean = false) {
        host.selection.clear()
        host.layout.cancelInteraction()
        try {
            display(book.session.locate(coordinate), host)
        } catch (_: InkunaException.NotReady) {
            pendingJump = coordinate
            runCatching { book.session.chapter(coordinate.spineIdx) }
        } catch (_: InkunaException.AnchorNotFound) {
            if (linkToast) notifyLinkFailed()
        } catch (failure: InkunaException) {
            if (linkToast) notifyLinkFailed()
        }
    }

    fun presentPending(host: EngineHost) {
        val coordinate = pendingJump ?: anchorState.value ?: viewModel.currentCoordinate() ?: return
        attemptJump(coordinate, host, pendingJump != null)
    }

    fun handleEvent(event: ReaderViewModel.LayoutEvent, host: EngineHost) {
        if (eventGeneration(event) == discardGeneration) return
        latestGeneration = eventGeneration(event)
        when (event) {
            is ReaderViewModel.LayoutEvent.FirstPage -> {
                host.surface.firstPageBecameReady(event.generation, event.spineIdx)
                viewModel.onFirstPageReady(event.spineIdx)
                if (anchorState.value == null) viewModel.resolveInitialLocation()?.let { display(it, host) }
                presentPending(host)
            }
            is ReaderViewModel.LayoutEvent.Chapter -> {
                host.surface.chapterBecameReady(event.generation, event.spineIdx)
                viewModel.onChapterReady(event.spineIdx)
                presentPending(host)
            }
            is ReaderViewModel.LayoutEvent.Failed -> {
                host.surface.chapterFailed(event.generation, event.spineIdx)
                if (pendingJump?.spineIdx == event.spineIdx || anchorState.value?.spineIdx == event.spineIdx) {
                    host.surface.display(event.spineIdx, 0u)
                }
            }
        }
    }

    val host = hostState.value
    if (host != null) {
        LaunchedEffect(book, host) {
            if (viewModel.consumeInitialHrefFailure()) notifyLinkFailed()
            book.initialLocation?.let { display(it, host) } ?: presentPending(host)
            viewModel.layoutEvents.collect { event ->
                if (relayoutInFlight) pendingEvents += event else handleEvent(event, host)
            }
        }
    }

    fun requestRelayout() {
        val live = hostState.value ?: return
        scope.launch {
            val anchor = anchorState.value ?: viewModel.currentCoordinate()
            val oldGeneration = latestGeneration
            relayoutInFlight = true
            live.selection.clear()
            live.layout.cancelInteraction()
            val updated = try {
                viewModel.updateAppearance(viewModel.settingsFor(snapshot))
            } finally {
                relayoutInFlight = false
            }
            if (updated) {
                discardGeneration = oldGeneration
                pendingJump = anchor
                live.surface.layoutInvalidated(0uL)
            }
            val buffered = pendingEvents.toList()
            pendingEvents.clear()
            buffered.forEach { event ->
                if (!updated || eventGeneration(event) != oldGeneration) handleEvent(event, live)
            }
        }
    }

    var appliedTypography by remember(book) { mutableStateOf(false) }
    var appliedViewport by remember(book) { mutableStateOf(false) }
    LaunchedEffect(
        snapshot.textSizeStep, snapshot.rawReadingFont, snapshot.readingBold, snapshot.lineSpacing,
        snapshot.letterSpacing, snapshot.wordSpacing, snapshot.readingMargins,
    ) {
        if (appliedTypography) requestRelayout() else appliedTypography = true
    }
    LaunchedEffect(configuration.screenWidthDp, configuration.screenHeightDp) {
        if (appliedViewport) requestRelayout() else appliedViewport = true
    }
    LaunchedEffect(toastCount) {
        if (toastCount > toastShown) {
            toastShown = toastCount
            toastVisible.value = true
            delay(1800)
            toastVisible.value = false
        }
    }
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

    Box(Modifier.fillMaxSize()) {
        AndroidView(
            factory = { viewContext ->
                ReaderFontStore.prime(book.session.fontRegistry())
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
                    viewModel.onPageSettled(spineIdx, pageIdx)
                    anchorState.value = runCatching {
                        Coordinate(spineIdx, book.session.pageCharRange(spineIdx, pageIdx).start)
                    }.getOrNull()
                    if (!touchExploration) {
                        chromeVisible.value = false
                        menuOpen.value = false
                    }
                }
                canvas.onPageDrawn = { spineIdx, pageIdx ->
                    if (surface.spineIdx == spineIdx && surface.pageIdx == pageIdx) viewModel.onCurrentPageDrawn()
                }
                canvas.onLinkActivated = { spineIdx, pageIdx, x, y ->
                    handleCanvasPoint(book, engineHost, x, y, ::notifyLinkFailed, { coordinate -> attemptJump(coordinate, engineHost, linkToast = true) }, chromeVisible, menuOpen)
                }
                canvas.onPageTap = { spineIdx, pageIdx, x, y ->
                    handleCanvasPoint(book, engineHost, x, y, ::notifyLinkFailed, { coordinate -> attemptJump(coordinate, engineHost, linkToast = true) }, chromeVisible, menuOpen)
                }
                layout.onTurnGesture = if (touchExploration) null else {
                    { chromeVisible.value = false; menuOpen.value = false }
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
                hit.charOffset?.let { offset -> hostState.value?.let { attemptJump(Coordinate(hit.spineIdx, offset), it) } }
                searchOpen.value = false
                chromeVisible.value = true
            },
            onCloseSearch = { searchOpen.value = false; chromeVisible.value = true },
            viewModel = viewModel,
        )
    }

    if (themeSheetOpen) {
        ThemeTypeSheet(snapshot, settings, onBrightnessPreview = { brightnessPreview = it }) { themeSheetOpen = false }
    }
    if (contentsSheetOpen) {
        ContentsSheet(
            publication = book.publication,
            chapters = book.chapters,
            positionRanges = book.positionRanges,
            currentPosition = anchorState.value?.let { runCatching { book.session.positionOf(it) }.getOrNull() },
            pageInfo = readerPageInfo(book, anchorState.value),
            onSelect = { chapter ->
                hostState.value?.let { live ->
                    runCatching { book.session.locateHrefParts(chapter.href) }
                        .onSuccess { attemptJump(it, live, linkToast = true) }
                        .onFailure { notifyLinkFailed() }
                }
            },
            onDismiss = { contentsSheetOpen = false },
        )
    }
}

private fun eventGeneration(event: ReaderViewModel.LayoutEvent): ULong = when (event) {
    is ReaderViewModel.LayoutEvent.FirstPage -> event.generation
    is ReaderViewModel.LayoutEvent.Chapter -> event.generation
    is ReaderViewModel.LayoutEvent.Failed -> event.generation
}

private fun handleCanvasPoint(
    book: ReaderViewModel.ReaderBook,
    host: EngineHost,
    x: Float,
    y: Float,
    onLinkFailed: () -> Unit,
    onInternalLink: (Coordinate) -> Unit,
    chromeVisible: MutableState<Boolean>,
    menuOpen: MutableState<Boolean>,
) {
    val hit = runCatching {
        book.session.hitTest(host.surface.spineIdx, host.surface.pageIdx, host.canvas.toLayoutX(x), host.canvas.toLayoutY(y))
    }.getOrNull()
    val target = hit?.linkTarget
    if (target != null) {
        val uri = target.toUri()
        if (uri.scheme == "http" || uri.scheme == "https") {
            runCatching { host.canvas.context.startActivity(Intent(Intent.ACTION_VIEW, uri)) }.onFailure { onLinkFailed() }
        } else {
            runCatching { book.session.locateHrefParts(target) }
                .onSuccess(onInternalLink)
                .onFailure { onLinkFailed() }
        }
        return
    }
    val band = maxOf(host.layout.width * 0.3f, 80f)
    when {
        x < band -> host.layout.turnBackward()
        x > host.layout.width - band -> host.layout.turnForward()
        else -> {
            menuOpen.value = false
            chromeVisible.value = !chromeVisible.value
        }
    }
}
