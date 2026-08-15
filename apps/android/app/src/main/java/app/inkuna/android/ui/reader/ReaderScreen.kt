package app.inkuna.android.ui.reader

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
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowBack
import androidx.compose.material.icons.automirrored.outlined.List
import androidx.compose.material.icons.filled.Bookmark
import androidx.compose.material.icons.outlined.Bookmark
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material.icons.outlined.FormatSize
import androidx.compose.material.icons.outlined.MoreHoriz
import androidx.compose.material.icons.outlined.Search
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.inkuna.android.R
import app.inkuna.android.model.AppSettings
import app.inkuna.android.model.PlaceholderBook
import app.inkuna.android.model.PlaceholderLibrary
import app.inkuna.android.ui.components.InkToast
import app.inkuna.android.ui.theme.InkMotion
import app.inkuna.android.ui.theme.InkType
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * The reader. Chrome shows on entry, hides when a page turn starts, and
 * toggles on a bare tap of the prose. All content is the placeholder
 * sample; TODO(core): Readium navigator replaces the pager.
 */
@Composable
fun ReaderScreen(
    book: PlaceholderBook,
    settings: AppSettings,
    snapshot: AppSettings.Snapshot,
    onBack: () -> Unit,
) {
    val scope = rememberCoroutineScope()
    val haptics = LocalHapticFeedback.current
    val theme = snapshot.readingTheme
    val fontSize = AppSettings.TEXT_SIZE_STEPS[snapshot.textSizeStep]

    val pagerState = rememberPagerState { PlaceholderLibrary.samplePages.size }
    var chromeVisible by rememberSaveable { mutableStateOf(true) }
    var menuOpen by rememberSaveable { mutableStateOf(false) }
    var themeSheetOpen by rememberSaveable { mutableStateOf(false) }
    var contentsSheetOpen by rememberSaveable { mutableStateOf(false) }
    var searchOpen by rememberSaveable { mutableStateOf(false) }
    var toastCount by rememberSaveable { mutableIntStateOf(0) }
    var toastShown by rememberSaveable { mutableIntStateOf(0) }
    var toastVisible by remember { mutableStateOf(false) }
    // A jump from a search result scrolls the pager programmatically; the
    // auto-hide below must not read that as a page turn.
    var jumping by remember { mutableStateOf(false) }
    var brightnessPreview by remember { mutableStateOf<Float?>(null) }

    // Under TalkBack a page-forward gesture *is* a scroll, so auto-hiding on
    // scroll would strand an exploring reader with no way back out.
    val context = LocalContext.current
    val touchExploration = remember(context) {
        context.getSystemService(AccessibilityManager::class.java)?.isTouchExplorationEnabled == true
    }
    val toggleChrome = {
        when {
            searchOpen -> {
                searchOpen = false
                chromeVisible = true
            }
            menuOpen -> menuOpen = false
            else -> chromeVisible = !chromeVisible
        }
    }
    val closeSearch = {
        searchOpen = false
        chromeVisible = true
    }

    // Back closes the reader's own layers before it leaves the book.
    BackHandler(enabled = searchOpen || menuOpen) {
        if (searchOpen) closeSearch() else menuOpen = false
    }

    val pageNumber = book.currentPage + pagerState.currentPage
    val percent = (pageNumber * 100f / book.pageCount).toInt()
    val pageInfo = stringResource(R.string.reader_page_info, pageNumber, book.pageCount, percent)

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

    // Starting a page turn tucks the chrome away.
    LaunchedEffect(pagerState, touchExploration) {
        snapshotFlow { pagerState.isScrollInProgress }.collect { scrolling ->
            if (scrolling && !jumping && !touchExploration) {
                chromeVisible = false
                menuOpen = false
            }
        }
    }

    // Toast lifecycle: repeated bookmarks replace the toast, not stack it.
    // The shown counter is saved alongside, so a rotation doesn't replay a
    // confirmation the reader already saw.
    LaunchedEffect(toastCount) {
        if (toastCount > toastShown) {
            toastShown = toastCount
            toastVisible = true
            delay(1800)
            toastVisible = false
        }
    }

    val statusPad = WindowInsets.statusBars.asPaddingValues().calculateTopPadding()
    val navPad = WindowInsets.navigationBars.asPaddingValues().calculateBottomPadding()

    Box(
        Modifier
            .fillMaxSize()
            .background(background)
    ) {
        val toggleLabel = stringResource(R.string.a11y_toggle_reader_controls)
        HorizontalPager(
            state = pagerState,
            modifier = Modifier
                .fillMaxSize()
                .pointerInput(Unit) { detectTapGestures { toggleChrome() } }
                // Raw pointer input carries no semantics, so TalkBack's
                // double-tap would have no way to reach the chrome.
                .semantics {
                    onClick(label = toggleLabel) {
                        toggleChrome()
                        true
                    }
                },
        ) { pageIndex ->
            ReaderPage(
                paragraphs = PlaceholderLibrary.samplePages[pageIndex],
                eyebrow = if (pageIndex == 0) PlaceholderLibrary.chapterEyebrow else null,
                lastPage = pageIndex == PlaceholderLibrary.samplePages.lastIndex,
                fontSize = fontSize,
                foreground = foreground,
                dimmed = foreground.copy(alpha = 0.55f),
                topPadding = statusPad + 64.dp,
            )
        }

        // Ink veil standing in for brightness — never the system backlight.
        // The preview keeps it tracking the slider while the drag is in
        // flight; the persisted value takes over once it lands.
        val brightness = brightnessPreview ?: snapshot.brightness
        val veil = (AppSettings.DEFAULT_BRIGHTNESS - brightness).coerceAtLeast(0f) / 1.7f
        if (veil > 0f) {
            Box(
                Modifier
                    .fillMaxSize()
                    .background(Color(0xFF0A0907).copy(alpha = veil))
            )
        }

        // Page-info footer, fading with the chrome.
        AnimatedVisibility(
            visible = chromeVisible,
            enter = fadeIn(tween(240, easing = InkMotion.easeQuiet)),
            exit = fadeOut(tween(240, easing = InkMotion.easeQuiet)),
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .padding(bottom = navPad + 4.dp),
        ) {
            Text(pageInfo, style = InkType.caption, color = foreground.copy(alpha = 0.55f))
        }

        // Back button.
        AnimatedVisibility(
            visible = chromeVisible && !searchOpen,
            enter = fadeIn(tween(240, easing = InkMotion.easeQuiet)),
            exit = fadeOut(tween(240, easing = InkMotion.easeQuiet)),
            modifier = Modifier
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
            visible = menuOpen,
            enter = fadeIn(tween(240, easing = InkMotion.easeQuiet)) +
                slideInVertically(tween(240, easing = InkMotion.easeQuiet)) { it / 10 },
            exit = fadeOut(tween(240, easing = InkMotion.easeQuiet)) +
                slideOutVertically(tween(240, easing = InkMotion.easeQuiet)) { it / 10 },
            modifier = Modifier
                .align(Alignment.BottomEnd)
                .padding(end = 16.dp, bottom = navPad + 26.dp + 46.dp + 12.dp),
        ) {
            Column(
                horizontalAlignment = Alignment.End,
                verticalArrangement = Arrangement.spacedBy(10.dp),
                modifier = Modifier.padding(start = 16.dp).widthIn(max = 320.dp),
            ) {
                ReaderMenuPill(
                    text = stringResource(R.string.reader_menu_contents, percent),
                    icon = Icons.AutoMirrored.Outlined.List,
                    onClick = {
                        menuOpen = false
                        contentsSheetOpen = true
                    },
                )
                ReaderMenuPill(
                    text = stringResource(R.string.reader_menu_theme_type),
                    icon = Icons.Outlined.FormatSize,
                    onClick = {
                        menuOpen = false
                        themeSheetOpen = true
                    },
                )
                Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    ReaderGlassButton(
                        icon = Icons.Outlined.Search,
                        contentDescription = stringResource(R.string.a11y_search_book),
                        onClick = {
                            menuOpen = false
                            chromeVisible = false
                            searchOpen = true
                        },
                    )
                    ReaderGlassButton(
                        icon = Icons.Outlined.Bookmark,
                        contentDescription = stringResource(R.string.a11y_place_bookmark),
                        onClick = {
                            // TODO(core): persist the bookmark.
                            haptics.performHapticFeedback(HapticFeedbackType.Confirm)
                            toastCount += 1
                        },
                    )
                }
            }
        }

        // Menu toggle.
        AnimatedVisibility(
            visible = chromeVisible && !searchOpen,
            enter = fadeIn(tween(240, easing = InkMotion.easeQuiet)),
            exit = fadeOut(tween(240, easing = InkMotion.easeQuiet)),
            modifier = Modifier
                .align(Alignment.BottomEnd)
                .padding(end = 16.dp, bottom = navPad + 26.dp),
        ) {
            ReaderGlassButton(
                icon = if (menuOpen) Icons.Outlined.Close else Icons.Outlined.MoreHoriz,
                contentDescription = stringResource(
                    if (menuOpen) R.string.a11y_close_reading_menu else R.string.a11y_reading_menu
                ),
                onClick = { menuOpen = !menuOpen },
            )
        }

        // Bookmark toast.
        AnimatedVisibility(
            visible = toastVisible,
            enter = fadeIn(tween(InkMotion.durFast, easing = InkMotion.easeQuiet)),
            exit = fadeOut(tween(InkMotion.durMed, easing = InkMotion.easeQuiet)),
            modifier = Modifier
                .align(Alignment.TopCenter)
                .padding(top = statusPad + 56.dp),
        ) {
            InkToast(
                text = stringResource(R.string.reader_bookmark_placed),
                icon = Icons.Filled.Bookmark,
            )
        }

        if (searchOpen) {
            // A scrim between the page and the panel: without a pointer-input
            // node here, taps and drags aimed at the panel's quiet areas fall
            // through and turn pages behind it.
            Box(
                Modifier
                    .fillMaxSize()
                    .pointerInput(Unit) { detectTapGestures { closeSearch() } }
            )
            ReaderSearchPanel(
                book = book,
                topPadding = statusPad + 8.dp,
                onJump = { pageIndex ->
                    scope.launch {
                        jumping = true
                        pagerState.scrollToPage(pageIndex)
                        jumping = false
                        chromeVisible = true
                    }
                    searchOpen = false
                },
                onClose = closeSearch,
            )
        }

        if (themeSheetOpen) {
            ThemeTypeSheet(
                snapshot = snapshot,
                settings = settings,
                onBrightnessPreview = { brightnessPreview = it },
                onDismiss = { themeSheetOpen = false },
            )
        }

        if (contentsSheetOpen) {
            ContentsSheet(
                book = book,
                pageInfo = pageInfo,
                onDismiss = { contentsSheetOpen = false },
            )
        }
    }
}

@Composable
private fun ReaderPage(
    paragraphs: List<String>,
    eyebrow: String?,
    lastPage: Boolean,
    fontSize: Float,
    foreground: Color,
    dimmed: Color,
    topPadding: androidx.compose.ui.unit.Dp,
) {
    val bodyStyle = InkType.reading.copy(
        fontSize = fontSize.sp,
        lineHeight = (fontSize * 1.65f).sp,
    )
    // The reading step multiplies with the system font scale, so a page can
    // outgrow its viewport. The pager clips its children, so overflow has to
    // scroll — otherwise the tail of every page is simply unreachable.
    // TODO(core): the Readium navigator repaginates instead of scrolling.
    Column(
        modifier = Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(start = 26.dp, end = 26.dp, top = topPadding, bottom = 70.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Column(Modifier.widthIn(max = 544.dp)) {
            if (eyebrow != null) {
                Text(
                    eyebrow.uppercase(java.util.Locale.getDefault()),
                    style = InkType.eyebrow,
                    color = dimmed,
                )
                Spacer(Modifier.height(18.dp))
            }
            paragraphs.forEachIndexed { index, paragraph ->
                val closing = lastPage && index == paragraphs.lastIndex
                Text(
                    paragraph,
                    style = if (closing) bodyStyle.copy(fontStyle = FontStyle.Italic) else bodyStyle,
                    color = if (closing) dimmed else foreground,
                    modifier = if (index == 0) Modifier
                    else Modifier.padding(top = (fontSize * 0.9f).dp),
                )
            }
        }
    }
}
