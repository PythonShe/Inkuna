package app.inkuna.android.ui.reader

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
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
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.State
import androidx.compose.ui.Alignment
import androidx.compose.ui.FrameRateCategory
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.preferredFrameRate
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.inkuna.android.R
import app.inkuna.android.ui.components.InkButton
import app.inkuna.android.ui.components.InkButtonSize
import app.inkuna.android.ui.components.InkToast
import app.inkuna.android.ui.theme.InkType
import app.inkuna.core.Coordinate
import kotlin.math.roundToInt

@Composable
internal fun ReaderChromeLayer(
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
    anchorState: State<Coordinate?>,
    onBack: () -> Unit,
    onOpenContents: () -> Unit,
    onOpenThemeType: () -> Unit,
    onPlaceBookmark: () -> Unit,
    onSelectSearch: (ReaderViewModel.SearchHit) -> Unit,
    onCloseSearch: () -> Unit,
    viewModel: ReaderViewModel,
) {
    // One position/count snapshot per recomposition: both the page-info
    // line and the menu percent read it, so the chrome crosses the FFI at
    // most twice (one positionOf + one positionCount) per pass.
    val position = anchorState.value?.let { runCatching { book.session.positionOf(it) }.getOrNull() }
    val positionCount = position?.let { book.session.positionCount() }
    Box(Modifier.fillMaxSize()) {
        AnimatedVisibility(chromeVisible.value, Modifier.preferredFrameRate(FrameRateCategory.High).align(Alignment.BottomCenter).padding(bottom = navPad + ReaderMetrics.footerLift), fadeIn(tween(240)), fadeOut(tween(240))) {
            Text(readerPageInfo(book, position, positionCount), style = InkType.caption, color = foreground.copy(alpha = 0.55f))
        }
        AnimatedVisibility(chromeVisible.value && !searchOpen.value, Modifier.align(Alignment.TopStart).padding(start = 16.dp, top = statusPad + 6.dp), fadeIn(tween(240)), fadeOut(tween(240))) {
            ReaderGlassButton(Icons.AutoMirrored.Outlined.ArrowBack, stringResource(R.string.a11y_back), onBack)
        }
        AnimatedVisibility(menuOpen.value, Modifier.align(Alignment.BottomEnd).padding(end = 16.dp, bottom = contentBottom + 58.dp), fadeIn(tween(240)) + slideInVertically(tween(240)) { it / 10 }, fadeOut(tween(240)) + slideOutVertically(tween(240)) { it / 10 }) {
            Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.widthIn(max = 320.dp)) {
                ReaderMenuPill(text = stringResource(R.string.reader_menu_contents, readerPercent(book, position, positionCount)), icon = Icons.AutoMirrored.Outlined.List, onClick = { menuOpen.value = false; onOpenContents() })
                ReaderMenuPill(text = stringResource(R.string.reader_menu_theme_type), icon = Icons.Outlined.FormatSize, onClick = { menuOpen.value = false; onOpenThemeType() })
                Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    ReaderGlassButton(icon = Icons.Outlined.Search, contentDescription = stringResource(R.string.a11y_search_book), onClick = { menuOpen.value = false; searchOpen.value = true })
                    ReaderGlassButton(Icons.Outlined.Bookmark, stringResource(R.string.a11y_place_bookmark), onPlaceBookmark)
                }
            }
        }
        AnimatedVisibility(chromeVisible.value && !searchOpen.value, Modifier.align(Alignment.BottomEnd).padding(end = 16.dp, bottom = contentBottom), fadeIn(tween(240)), fadeOut(tween(240))) {
            ReaderGlassButton(icon = if (menuOpen.value) Icons.Outlined.Close else Icons.Outlined.MoreHoriz, contentDescription = stringResource(if (menuOpen.value) R.string.a11y_close_reading_menu else R.string.a11y_reading_menu), onClick = { menuOpen.value = !menuOpen.value })
        }
        AnimatedVisibility(toastVisible.value, Modifier.align(Alignment.TopCenter).padding(top = statusPad + 56.dp), fadeIn(), fadeOut()) {
            InkToast(stringResource(if (toastMessage.value == ReaderToast.BookmarkPlaced) R.string.reader_bookmark_placed else R.string.reader_link_failed), if (toastMessage.value == ReaderToast.BookmarkPlaced) Icons.Filled.Bookmark else Icons.Outlined.LinkOff)
        }
        if (searchOpen.value) {
            Box(Modifier.fillMaxSize().pointerInput(Unit) { detectTapGestures { onCloseSearch() } })
            ReaderSearchPanel(statusPad + 8.dp, viewModel, onSelectSearch, onCloseSearch)
        }
    }
}

@Composable
internal fun ReaderOpenFailed(
    foreground: Color,
    message: String,
    onRetry: (() -> Unit)?,
    modifier: Modifier = Modifier,
) {
    Column(horizontalAlignment = Alignment.CenterHorizontally, modifier = modifier.padding(horizontal = 40.dp)) {
        Text(message, style = InkType.reading, color = foreground.copy(alpha = 0.75f))
        if (onRetry != null) {
            Spacer(Modifier.height(18.dp))
            InkButton(stringResource(R.string.reader_retry), onRetry, size = InkButtonSize.Small)
        }
    }
}

/** Pure percent over an already-read position/count pair; no FFI crossings. */
private fun readerPercent(book: ReaderViewModel.ReaderBook, position: UInt?, count: UInt?): Int {
    if (position == null || count == null) return (book.publication.progression * 100).roundToInt().coerceIn(0, 100)
    return (position.toDouble() / count.coerceAtLeast(1u).toDouble() * 100).roundToInt().coerceIn(0, 100)
}

@Composable
internal fun readerPageInfo(book: ReaderViewModel.ReaderBook, position: UInt?, count: UInt?): String =
    if (position != null && count != null) {
        val args = ReaderPositionFormat.resourceArgs(position, count)
        stringResource(R.string.reader_page_info, args[0], args[1], readerPercent(book, position, count))
    } else {
        stringResource(R.string.reader_percent, readerPercent(book, null, null))
    }

/** Convenience over the snapshot form for callers holding only a coordinate. */
@Composable
internal fun readerPageInfo(book: ReaderViewModel.ReaderBook, coordinate: Coordinate?): String {
    val position = coordinate?.let { runCatching { book.session.positionOf(it) }.getOrNull() }
    return readerPageInfo(book, position, position?.let { book.session.positionCount() })
}

internal object ReaderPositionFormat {
    fun resourceArgs(position: UInt, count: UInt): List<Int> = listOf(position.toInt(), count.toInt())
}

internal enum class ReaderToast { BookmarkPlaced, LinkFailed }
