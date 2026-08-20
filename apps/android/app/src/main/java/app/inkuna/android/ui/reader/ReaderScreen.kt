package app.inkuna.android.ui.reader

import android.content.Intent
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
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.State
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
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.preferredFrameRate
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.max
import androidx.core.net.toUri
import androidx.lifecycle.compose.LifecycleStartEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.inkuna.android.R
import app.inkuna.android.model.AppSettings
import app.inkuna.android.ui.components.InkButton
import app.inkuna.android.ui.components.InkButtonSize
import app.inkuna.android.ui.components.InkToast
import app.inkuna.android.ui.theme.InkMotion
import app.inkuna.android.ui.theme.InkType
import kotlin.math.roundToInt
import kotlinx.coroutines.delay
import org.readium.r2.navigator.epub.EpubNavigatorFragment
import org.readium.r2.navigator.epub.EpubPreferences
import org.readium.r2.navigator.input.InputListener
import org.readium.r2.navigator.input.TapEvent
import org.readium.r2.navigator.preferences.Color as ReadiumColor
import org.readium.r2.navigator.preferences.Theme as ReadiumTheme
import org.readium.r2.shared.ExperimentalReadiumApi
import org.readium.r2.shared.publication.Locator
import org.readium.r2.shared.util.AbsoluteUrl

/**
 * The reader: Readium's EPUB navigator rendering the core-owned file, with
 * the shell's floating chrome above it. Chrome shows on entry, hides when a
 * page turns, and toggles on a bare tap of the prose. The core stores the
 * position (locator + progression) and the sessions; Readium owns rendering
 * and pagination.
 */
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

    val background by animateColorAsState(
        theme.background,
        tween(InkMotion.durMed, easing = InkMotion.easeQuiet),
        label = "readerBg",
    )
    val foreground by animateColorAsState(
        theme.foreground,
        tween(InkMotion.durMed, easing = InkMotion.easeQuiet),
        label = "readerFg",
    )

    val statusPad = WindowInsets.statusBars.asPaddingValues().calculateTopPadding()
    val navPad = WindowInsets.navigationBars.asPaddingValues().calculateBottomPadding()

    Box(
        Modifier
            .fillMaxSize()
            .background(background)
    ) {
        when (val current = state) {
            is ReaderViewModel.UiState.Opening -> {
                // Quiet while the book opens — a spinner would be louder
                // than the beat it takes.
            }
            is ReaderViewModel.UiState.Failed -> {
                ReaderOpenFailed(
                    foreground = foreground,
                    onRetry = viewModel::open,
                    modifier = Modifier.align(Alignment.Center),
                )
            }
            is ReaderViewModel.UiState.Ready -> {
                ReaderContent(
                    viewModel = viewModel,
                    book = current.book,
                    settings = settings,
                    snapshot = snapshot,
                    foreground = foreground,
                    statusPad = statusPad,
                    navPad = navPad,
                    onBack = onBack,
                )
            }
        }

        // The back affordance survives every state — a book that will not
        // open must never trap the reader.
        if (state !is ReaderViewModel.UiState.Ready) {
            Box(
                Modifier
                    .align(Alignment.TopStart)
                    .padding(start = 16.dp, top = statusPad + 6.dp)
            ) {
                ReaderGlassButton(
                    icon = Icons.AutoMirrored.Outlined.ArrowBack,
                    contentDescription = stringResource(R.string.a11y_back),
                    onClick = onBack,
                )
            }
        }
    }
}

@OptIn(ExperimentalReadiumApi::class)
@Composable
private fun ReaderContent(
    viewModel: ReaderViewModel,
    book: ReaderViewModel.ReaderBook,
    settings: AppSettings,
    snapshot: AppSettings.Snapshot,
    foreground: Color,
    statusPad: androidx.compose.ui.unit.Dp,
    navPad: androidx.compose.ui.unit.Dp,
    onBack: () -> Unit,
) {
    val haptics = LocalHapticFeedback.current
    val context = LocalContext.current

    // Chrome state lives in State objects handed to ReaderChromeLayer,
    // which reads them inside its own recomposition scope: a bare read in
    // this body would recompose all of ReaderContent on every page turn
    // (every turn hides the chrome), re-reading insets and re-running
    // every subtree on exactly the frames the page is moving.
    val chromeVisible = rememberSaveable { mutableStateOf(true) }
    val menuOpen = rememberSaveable { mutableStateOf(false) }
    var themeSheetOpen by rememberSaveable { mutableStateOf(false) }
    var contentsSheetOpen by rememberSaveable { mutableStateOf(false) }
    val searchOpen = rememberSaveable { mutableStateOf(false) }
    var toastCount by rememberSaveable { mutableIntStateOf(0) }
    var toastShown by rememberSaveable { mutableIntStateOf(0) }
    val toastVisible = remember { mutableStateOf(false) }
    // One toast slot, two things worth saying: the bookmark confirmation and
    // a link the shell would not follow. Saved as an enum, never as a raw
    // resource id — ids are not stable across app updates, and a restored
    // stale id would throw at the stringResource call.
    val toastMessage = rememberSaveable { mutableStateOf(ReaderToast.BookmarkPlaced) }
    // A programmatic jump (contents sheet) moves the locator; the auto-hide
    // below must not read that as a page turn.
    var jumping by remember { mutableStateOf(false) }
    var brightnessPreview by remember { mutableStateOf<Float?>(null) }

    var navigator by remember { mutableStateOf<EpubNavigatorFragment?>(null) }
    // Held as a State the chrome reads inside its own scopes: a bare read
    // in this body would recompose all of ReaderContent on every locator
    // emission, and Readium emits several per page turn.
    val locatorState = remember { mutableStateOf(book.initialLocator) }

    // Under TalkBack a page-forward gesture *is* a scroll, so auto-hiding on
    // page turns would strand an exploring reader with no way back out.
    val touchExploration = remember(context) {
        context.getSystemService(AccessibilityManager::class.java)?.isTouchExplorationEnabled == true
    }
    // Remembered so the navigator host's modifier (below) stays stable
    // across recompositions; the bodies read the state they need when run.
    val toggleChrome = remember {
        {
            when {
                searchOpen.value -> {
                    searchOpen.value = false
                    chromeVisible.value = true
                }
                menuOpen.value -> menuOpen.value = false
                else -> chromeVisible.value = !chromeVisible.value
            }
        }
    }
    val closeSearch = remember {
        {
            searchOpen.value = false
            chromeVisible.value = true
        }
    }

    // Back closes the reader's own layers before it leaves the book.
    BackHandler(enabled = searchOpen.value || menuOpen.value) {
        if (searchOpen.value) closeSearch() else menuOpen.value = false
    }

    // Reading sessions bracket the reader's visible lifetime — leaving the
    // book and backgrounding the app both end the sitting; they power Stats.
    LifecycleStartEffect(book) {
        viewModel.onReaderVisible()
        onStopOrDispose { viewModel.onReaderHidden() }
    }

    val navigatorListener = remember(book) {
        object : EpubNavigatorFragment.Listener {
            /**
             * Only http(s) leaves the app. An EPUB is untrusted content, and
             * `intent:`, `market:`, `tel:` and friends would let a book aim
             * an implicit intent at anything installed; the reader is told
             * the link was not followed instead of it firing silently.
             */
            override fun onExternalLinkActivated(url: AbsoluteUrl) {
                val opened = url.isHttp && runCatching {
                    context.startActivity(Intent(Intent.ACTION_VIEW, url.toString().toUri()))
                }.isSuccess
                if (!opened) {
                    toastMessage.value = ReaderToast.LinkFailed
                    toastCount += 1
                }
            }
        }
    }

    val toggleLabel = stringResource(R.string.a11y_toggle_reader_controls)
    // The preferences the fragment is built with. Remembered so the effect
    // below can tell a real change from its own first run.
    val initialPreferences = remember(book) { readingPreferences(snapshot) }
    // The shared reading band (ReaderMetrics, mirrored by the iOS shell):
    // phones pad off the larger of the status-bar and cutout insets so a
    // notch never crowds the text; tablets get the fixed generous band.
    // Readium's own insets padding is disabled in ReaderNavigatorHost, so
    // this is the one and only vertical padding the page gets.
    val cutoutTop = WindowInsets.displayCutout.asPaddingValues().calculateTopPadding()
    val isTablet = LocalConfiguration.current.smallestScreenWidthDp >= 600
    val contentTop = ReaderMetrics.contentTop(max(statusPad, cutoutTop), isTablet)
    val contentBottom = ReaderMetrics.contentBottom(navPad, isTablet)
    // Hoisted so the fragment host's modifier is one stable value: rebuilt
    // per recomposition, the fresh semantics lambda would re-update the
    // AndroidView's modifier node on every page turn.
    val hostModifier = remember(contentTop, contentBottom, toggleLabel, toggleChrome) {
        Modifier
            .fillMaxSize()
            .padding(top = contentTop, bottom = contentBottom)
            // The WebView owns raw touch; TalkBack still needs a named way
            // to reach the chrome.
            .semantics {
                onClick(label = toggleLabel) {
                    toggleChrome()
                    true
                }
            }
    }
    // The pager layout drives every page turn; held so taps, keys, and
    // jumps can route through its springs.
    val pagerLayout = remember { mutableStateOf<ReaderPagerLayout?>(null) }
    ReaderNavigatorHost(
        navigatorFactory = book.navigatorFactory,
        initialLocator = book.initialLocator,
        initialPreferences = initialPreferences,
        listener = navigatorListener,
        onPager = { pagerLayout.value = it },
        onNavigator = { navigator = it },
        modifier = hostModifier,
    )

    // Chrome leaves the moment a page-turn drag is claimed — before the
    // motion — matching iOS; the locator collect below stays as the
    // fallback for programmatic turns. Under TalkBack a page gesture is
    // a scroll, so the chrome must not vanish on it.
    LaunchedEffect(pagerLayout.value, touchExploration) {
        pagerLayout.value?.onTurnGesture = if (touchExploration) {
            null
        } else {
            {
                chromeVisible.value = false
                menuOpen.value = false
            }
        }
    }

    // The design system's reading themes and type scale, routed through
    // Readium's user preferences instead of fighting the navigator. The
    // fragment was just built with [initialPreferences]; re-submitting the
    // same values would re-inject CSS into every freshly created WebView.
    var submittedPreferences by remember(book) { mutableStateOf(initialPreferences) }
    LaunchedEffect(navigator, snapshot.readingTheme, snapshot.textSizeStep) {
        val nav = navigator ?: return@LaunchedEffect
        val preferences = readingPreferences(snapshot)
        if (preferences == submittedPreferences) return@LaunchedEffect
        submittedPreferences = preferences
        nav.submitPreferences(preferences)
    }

    // Edge taps and hardware keys turn pages through the pager's springs
    // (a tapped boundary turn slides the neighbour in, exactly like a
    // dragged one); everything else toggles the chrome.
    DisposableEffect(navigator) {
        val nav = navigator ?: return@DisposableEffect onDispose {}
        val pageTurns = ReaderPageTurnListener(nav, pagerLayout::value)
        val chromeTaps = object : InputListener {
            override fun onTap(event: TapEvent): Boolean {
                toggleChrome()
                return true
            }
        }
        nav.addInputListener(pageTurns)
        nav.addInputListener(chromeTaps)
        onDispose {
            nav.removeInputListener(pageTurns)
            nav.removeInputListener(chromeTaps)
        }
    }

    // One updateProgress per page turn; a turn also tucks the chrome away.
    LaunchedEffect(navigator, touchExploration) {
        val nav = navigator ?: return@LaunchedEffect
        nav.currentLocator.collect { current ->
            val previous = locatorState.value
            locatorState.value = current
            viewModel.onLocatorChanged(current)
            // A turn is a move between two *known* places. The navigator's
            // early emissions refine the restored locator (filling in the
            // position it did not have yet); hiding the chrome on those
            // would strip a reader who only just arrived.
            val turned = previous?.locations?.position != null &&
                current.locations.position != null && (
                current.href != previous.href ||
                    current.locations.position != previous.locations.position
                )
            when {
                jumping -> {
                    jumping = false
                    chromeVisible.value = true
                }
                turned && !touchExploration -> {
                    chromeVisible.value = false
                    menuOpen.value = false
                }
            }
        }
    }

    val jumpTo: (Locator) -> Unit = remember {
        { target ->
            navigator?.let { nav ->
                // A turn mid-flight must not land on top of the jump.
                pagerLayout.value?.cancelInteraction()
                jumping = true
                nav.go(target)
                chromeVisible.value = true
            }
        }
    }

    // Toast lifecycle: repeated bookmarks replace the toast, not stack it.
    // The shown counter is saved alongside, so a rotation doesn't replay a
    // confirmation the reader already saw.
    LaunchedEffect(toastCount) {
        if (toastCount > toastShown) {
            toastShown = toastCount
            toastVisible.value = true
            delay(1800)
            toastVisible.value = false
        }
    }

    // Ink veil standing in for brightness — never the system backlight.
    // The preview keeps it tracking the slider while the drag is in
    // flight; the persisted value takes over once it lands. The preview —
    // the hot path — is read in the draw phase, so a slider drag
    // invalidates drawing alone, never composition (the persisted value
    // is a body read; a committed change recomposes once, on release).
    // An idle veil draws nothing at all.
    val snapshotBrightness = snapshot.brightness
    Box(
        Modifier
            .fillMaxSize()
            .drawBehind {
                val brightness = brightnessPreview ?: snapshotBrightness
                val veil =
                    (AppSettings.DEFAULT_BRIGHTNESS - brightness).coerceAtLeast(0f) / 1.7f
                if (veil > 0f) drawRect(Color(0xFF0A0907).copy(alpha = veil))
            }
    )

    // The bookmark write path stays here: it drives the toast counters,
    // which belong to ReaderContent's one-slot toast machinery.
    val placeBookmark = remember(book) {
        {
            navigator?.currentLocator?.value?.let { at ->
                viewModel.addBookmark(at) {
                    haptics.performHapticFeedback(HapticFeedbackType.Confirm)
                    toastMessage.value = ReaderToast.BookmarkPlaced
                    toastCount += 1
                }
            }
            Unit
        }
    }

    ReaderChromeLayer(
        viewModel = viewModel,
        book = book,
        foreground = foreground,
        statusPad = statusPad,
        navPad = navPad,
        contentBottom = contentBottom,
        chromeVisible = chromeVisible,
        menuOpen = menuOpen,
        searchOpen = searchOpen,
        toastVisible = toastVisible,
        toastMessage = toastMessage,
        locatorState = locatorState,
        onBack = onBack,
        onOpenContents = { contentsSheetOpen = true },
        onOpenThemeType = { themeSheetOpen = true },
        onPlaceBookmark = placeBookmark,
        onJump = jumpTo,
        onCloseSearch = closeSearch,
    )

    if (themeSheetOpen) {
        ThemeTypeSheet(
            snapshot = snapshot,
            settings = settings,
            onBrightnessPreview = { brightnessPreview = it },
            onDismiss = { themeSheetOpen = false },
        )
    }

    if (contentsSheetOpen) {
        val locator = locatorState.value
        val currentChapterIndex = remember(locator, book) {
            viewModel.currentChapterIndex(locator)
        }
        ContentsSheet(
            publication = book.core,
            chapters = book.chapters,
            currentChapterIndex = currentChapterIndex,
            pageInfo = readerPageInfo(locator, book),
            onSelect = { chapter -> viewModel.chapterLocator(chapter)?.let(jumpTo) },
            onDismiss = { contentsSheetOpen = false },
        )
    }
}

/**
 * Every floating layer above the page — footer, glass buttons, speed-dial,
 * toast, search — in its own recomposition scope. The chrome states are
 * read HERE, never in [ReaderContent]'s body: every page turn flips
 * [chromeVisible], and reading it one level up would recompose the whole
 * reader on exactly the frames the page is moving.
 */
@Composable
private fun ReaderChromeLayer(
    viewModel: ReaderViewModel,
    book: ReaderViewModel.ReaderBook,
    foreground: Color,
    statusPad: Dp,
    navPad: Dp,
    contentBottom: Dp,
    chromeVisible: MutableState<Boolean>,
    menuOpen: MutableState<Boolean>,
    searchOpen: MutableState<Boolean>,
    toastVisible: State<Boolean>,
    toastMessage: State<ReaderToast>,
    locatorState: State<Locator?>,
    onBack: () -> Unit,
    onOpenContents: () -> Unit,
    onOpenThemeType: () -> Unit,
    onPlaceBookmark: () -> Unit,
    onJump: (Locator) -> Unit,
    onCloseSearch: () -> Unit,
) {
    Box(Modifier.fillMaxSize()) {
        // Page-info footer, fading with the chrome.
        AnimatedVisibility(
            visible = chromeVisible.value,
            enter = fadeIn(tween(240, easing = InkMotion.easeQuiet)),
            exit = fadeOut(tween(240, easing = InkMotion.easeQuiet)),
            modifier = Modifier
                // The chrome's fades run over a page that scrolls at the
                // display's full rate; without their own votes they'd run
                // in the 60 Hz category and read as stutter against it.
                .preferredFrameRate(FrameRateCategory.High)
                .align(Alignment.BottomCenter)
                .padding(bottom = navPad + ReaderMetrics.footerLift),
        ) {
            // locatorState is read here, inside this content lambda, so a
            // page turn recomposes the footer text alone.
            Text(
                readerPageInfo(locatorState.value, book),
                style = InkType.caption,
                color = foreground.copy(alpha = 0.55f),
            )
        }

        // Back button.
        AnimatedVisibility(
            visible = chromeVisible.value && !searchOpen.value,
            enter = fadeIn(tween(240, easing = InkMotion.easeQuiet)),
            exit = fadeOut(tween(240, easing = InkMotion.easeQuiet)),
            modifier = Modifier
                .preferredFrameRate(FrameRateCategory.High)
                .align(Alignment.TopStart)
                .padding(start = 16.dp, top = statusPad + 6.dp),
        ) {
            ReaderGlassButton(
                icon = Icons.AutoMirrored.Outlined.ArrowBack,
                contentDescription = stringResource(R.string.a11y_back),
                onClick = onBack,
            )
        }

        // Speed-dial reading menu.
        AnimatedVisibility(
            visible = menuOpen.value,
            enter = fadeIn(tween(240, easing = InkMotion.easeQuiet)) +
                slideInVertically(tween(240, easing = InkMotion.easeQuiet)) { it / 10 },
            exit = fadeOut(tween(240, easing = InkMotion.easeQuiet)) +
                slideOutVertically(tween(240, easing = InkMotion.easeQuiet)) { it / 10 },
            modifier = Modifier
                .preferredFrameRate(FrameRateCategory.High)
                .align(Alignment.BottomEnd)
                .padding(end = 16.dp, bottom = contentBottom + 46.dp + 12.dp),
        ) {
            Column(
                horizontalAlignment = Alignment.End,
                verticalArrangement = Arrangement.spacedBy(10.dp),
                modifier = Modifier.padding(start = 16.dp).widthIn(max = 320.dp),
            ) {
                ReaderMenuPill(
                    text = stringResource(
                        R.string.reader_menu_contents,
                        readerPercent(locatorState.value, book),
                    ),
                    icon = Icons.AutoMirrored.Outlined.List,
                    onClick = {
                        menuOpen.value = false
                        onOpenContents()
                    },
                )
                ReaderMenuPill(
                    text = stringResource(R.string.reader_menu_theme_type),
                    icon = Icons.Outlined.FormatSize,
                    onClick = {
                        menuOpen.value = false
                        onOpenThemeType()
                    },
                )
                Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    ReaderGlassButton(
                        icon = Icons.Outlined.Search,
                        contentDescription = stringResource(R.string.a11y_search_book),
                        onClick = {
                            menuOpen.value = false
                            chromeVisible.value = false
                            searchOpen.value = true
                        },
                    )
                    ReaderGlassButton(
                        icon = Icons.Outlined.Bookmark,
                        contentDescription = stringResource(R.string.a11y_place_bookmark),
                        onClick = onPlaceBookmark,
                    )
                }
            }
        }

        // Menu toggle.
        AnimatedVisibility(
            visible = chromeVisible.value && !searchOpen.value,
            enter = fadeIn(tween(240, easing = InkMotion.easeQuiet)),
            exit = fadeOut(tween(240, easing = InkMotion.easeQuiet)),
            modifier = Modifier
                .preferredFrameRate(FrameRateCategory.High)
                .align(Alignment.BottomEnd)
                .padding(end = 16.dp, bottom = contentBottom),
        ) {
            ReaderGlassButton(
                icon = if (menuOpen.value) Icons.Outlined.Close else Icons.Outlined.MoreHoriz,
                contentDescription = stringResource(
                    if (menuOpen.value) {
                        R.string.a11y_close_reading_menu
                    } else {
                        R.string.a11y_reading_menu
                    }
                ),
                onClick = { menuOpen.value = !menuOpen.value },
            )
        }

        // Bookmark / blocked-link toast.
        AnimatedVisibility(
            visible = toastVisible.value,
            enter = fadeIn(tween(InkMotion.durFast, easing = InkMotion.easeQuiet)),
            exit = fadeOut(tween(InkMotion.durMed, easing = InkMotion.easeQuiet)),
            modifier = Modifier
                .preferredFrameRate(FrameRateCategory.High)
                .align(Alignment.TopCenter)
                .padding(top = statusPad + 56.dp),
        ) {
            InkToast(
                text = stringResource(
                    when (toastMessage.value) {
                        ReaderToast.BookmarkPlaced -> R.string.reader_bookmark_placed
                        ReaderToast.LinkFailed -> R.string.reader_link_failed
                    },
                ),
                icon = when (toastMessage.value) {
                    ReaderToast.BookmarkPlaced -> Icons.Filled.Bookmark
                    ReaderToast.LinkFailed -> Icons.Outlined.LinkOff
                },
            )
        }

        if (searchOpen.value) {
            // A scrim between the page and the panel: without a pointer-input
            // node here, taps and drags aimed at the panel's quiet areas fall
            // through to the navigator behind it.
            Box(
                Modifier
                    .fillMaxSize()
                    .pointerInput(Unit) { detectTapGestures { onCloseSearch() } }
            )
            ReaderSearchPanel(
                topPadding = statusPad + 8.dp,
                viewModel = viewModel,
                onSelect = { hit ->
                    // The jump lands first, then the panel gets out of the
                    // way — closing first would hand the navigator a target
                    // while the chrome is still animating over it.
                    viewModel.searchLocator(hit)?.let(onJump)
                    onCloseSearch()
                },
                onClose = onCloseSearch,
            )
        }
    }
}

@Composable
private fun ReaderOpenFailed(
    foreground: Color,
    onRetry: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        modifier = modifier.padding(horizontal = 40.dp),
    ) {
        Text(
            stringResource(R.string.reader_open_failed),
            style = InkType.reading,
            color = foreground.copy(alpha = 0.75f),
        )
        Spacer(Modifier.height(18.dp))
        InkButton(
            text = stringResource(R.string.reader_retry),
            onClick = onRetry,
            size = InkButtonSize.Small,
        )
    }
}

/** Whole-book percentage at [locator], falling back to the core's saved
 *  progression until the navigator has said where it is. */
private fun readerPercent(locator: Locator?, book: ReaderViewModel.ReaderBook): Int {
    val progression = locator?.locations?.totalProgression ?: book.core.progression
    return (progression * 100).roundToInt().coerceIn(0, 100)
}

/** Honest numbers: Readium's synthetic positions, never invented pages.
 *  Deliberately not restartable — the caller's scope owns the state read. */
@Composable
private fun readerPageInfo(locator: Locator?, book: ReaderViewModel.ReaderBook): String {
    val position = locator?.locations?.position
    val percent = readerPercent(locator, book)
    return if (position != null && book.positionCount > 0) {
        stringResource(R.string.reader_page_info, position, book.positionCount, percent)
    } else {
        stringResource(R.string.reader_percent, percent)
    }
}

/**
 * The design system's reading surface, spoken in Readium preferences: the
 * theme's exact ink and ground, and the 0.9–1.25rem type scale as a
 * multiplier of the publisher size. Vertical writing stays untouched —
 * Readium derives it from the publication language.
 */
@OptIn(ExperimentalReadiumApi::class)
private fun readingPreferences(snapshot: AppSettings.Snapshot): EpubPreferences {
    val theme = snapshot.readingTheme
    return EpubPreferences(
        theme = if (theme.isNight) ReadiumTheme.DARK else ReadiumTheme.LIGHT,
        backgroundColor = ReadiumColor(theme.background.toArgb()),
        textColor = ReadiumColor(theme.foreground.toArgb()),
        fontSize = AppSettings.TEXT_SIZE_STEPS[snapshot.textSizeStep] / 16.0,
    )
}

/** What the reader's single toast slot is currently saying. */
private enum class ReaderToast { BookmarkPlaced, LinkFailed }
