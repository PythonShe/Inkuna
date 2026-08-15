package app.inkuna.android.ui.reader

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Search
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.inkuna.android.R
import app.inkuna.android.model.PlaceholderBook
import app.inkuna.android.model.PlaceholderLibrary
import app.inkuna.android.ui.components.glassModifier
import app.inkuna.android.ui.stats.hairlineThickness
import app.inkuna.android.ui.theme.InkRadius
import app.inkuna.android.ui.theme.InkTheme
import app.inkuna.android.ui.theme.InkType

private data class SearchHit(val pageIndex: Int, val snippet: String)

/**
 * A single Han, Kana or Hangul character is a whole word — 月 and 书 are
 * real queries — so the two-character floor is a Latin rule and must not
 * apply to CJK text.
 */
private fun isCjk(text: String): Boolean = text.any { char ->
    when (Character.UnicodeScript.of(char.code)) {
        Character.UnicodeScript.HAN,
        Character.UnicodeScript.HIRAGANA,
        Character.UnicodeScript.KATAKANA,
        Character.UnicodeScript.HANGUL -> true
        else -> false
    }
}

/** Queries shorter than this match too much to be useful — Latin only. */
internal fun isSearchable(query: String): Boolean {
    val trimmed = query.trim()
    return trimmed.length > 1 || (trimmed.isNotEmpty() && isCjk(trimmed))
}

/**
 * In-book search over the sample pages: first match per page.
 * TODO(core): route through the core's CJK-aware search index.
 */
private fun search(query: String): List<SearchHit> {
    val trimmed = query.trim()
    if (!isSearchable(trimmed)) return emptyList()
    return PlaceholderLibrary.samplePages.mapIndexedNotNull { index, paragraphs ->
        val text = paragraphs.joinToString(" ")
        val at = text.indexOf(trimmed, ignoreCase = true)
        if (at < 0) return@mapIndexedNotNull null
        val start = (at - 34).coerceAtLeast(0)
        val end = (at + trimmed.length + 46).coerceAtMost(text.length)
        val snippet = buildString {
            if (start > 0) append("…")
            append(text.substring(start, end).trim())
            if (end < text.length) append("…")
        }
        SearchHit(index, snippet)
    }
}

@Composable
fun ReaderSearchPanel(
    book: PlaceholderBook,
    topPadding: Dp,
    onJump: (Int) -> Unit,
    onClose: () -> Unit,
) {
    val ink = InkTheme.colors
    var query by rememberSaveable { mutableStateOf("") }
    val results = remember(query) { search(query) }
    val focusRequester = remember { FocusRequester() }
    val placeholder = stringResource(R.string.reader_search_placeholder)

    LaunchedEffect(Unit) { focusRequester.requestFocus() }

    val focusManager = LocalFocusManager.current
    Box(
        Modifier
            .fillMaxWidth()
            .padding(start = 14.dp, end = 14.dp, top = topPadding)
            .imePadding()
            // Taps on the panel's quiet areas stop here; without this they
            // reach the scrim behind and dismiss the search mid-typing.
            .pointerInput(Unit) { detectTapGestures { } }
    ) {
        Column(
            Modifier
                .then(glassModifier(InkRadius.lgShape))
                .padding(horizontal = 16.dp, vertical = 14.dp)
        ) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                Icon(
                    Icons.Outlined.Search,
                    contentDescription = null,
                    tint = ink.textTertiary,
                    modifier = Modifier.size(20.dp),
                )
                val textStyle = InkType.ui.copy(fontWeight = FontWeight.Normal, color = ink.textDisplay)
                BasicTextField(
                    value = query,
                    onValueChange = { query = it },
                    textStyle = textStyle,
                    singleLine = true,
                    cursorBrush = SolidColor(ink.accent),
                    keyboardOptions = KeyboardOptions(
                        imeAction = ImeAction.Search,
                        autoCorrectEnabled = false,
                    ),
                    keyboardActions = KeyboardActions(onSearch = { focusManager.clearFocus() }),
                    modifier = Modifier
                        .weight(1f)
                        .focusRequester(focusRequester)
                        .semantics { contentDescription = placeholder },
                    decorationBox = { inner ->
                        Box(Modifier.padding(vertical = 8.dp)) {
                            if (query.isEmpty()) {
                                Text(placeholder, style = textStyle, color = ink.textTertiary)
                            }
                            inner()
                        }
                    },
                )
                SheetCloseButton(onClick = onClose)
            }
            if (isSearchable(query)) {
                Box(
                    Modifier
                        .fillMaxWidth()
                        .padding(top = 10.dp)
                        .height(hairlineThickness())
                        .background(ink.borderHairline)
                )
                val countLabel = if (results.isEmpty()) {
                    stringResource(R.string.a11y_no_results)
                } else {
                    pluralStringResource(R.plurals.a11y_result_count, results.size, results.size)
                }
                if (results.isEmpty()) {
                    Text(
                        stringResource(R.string.reader_search_empty),
                        style = InkType.reading.copy(fontSize = 15.sp, lineHeight = 22.sp),
                        color = ink.textTertiary,
                        textAlign = TextAlign.Center,
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(top = 18.dp, bottom = 8.dp)
                            .semantics {
                                liveRegion = LiveRegionMode.Polite
                                contentDescription = countLabel
                            },
                    )
                } else {
                    LazyColumn(
                        modifier = Modifier
                            .padding(top = 4.dp)
                            .heightInPanel()
                            .semantics { liveRegion = LiveRegionMode.Polite },
                    ) {
                        itemsIndexed(results, key = { _, hit -> hit.pageIndex }) { _, hit ->
                            Column(
                                Modifier
                                    .fillMaxWidth()
                                    .clickable { onJump(hit.pageIndex) }
                                    .padding(horizontal = 4.dp, vertical = 11.dp)
                            ) {
                                Text(
                                    hit.snippet,
                                    style = InkType.reading.copy(fontSize = 15.sp, lineHeight = 22.sp),
                                    color = ink.textDisplay,
                                    maxLines = 2,
                                    overflow = TextOverflow.Ellipsis,
                                )
                                Text(
                                    stringResource(
                                        R.string.reader_chapter_page,
                                        book.currentPage + hit.pageIndex,
                                    ),
                                    style = InkType.caption,
                                    color = ink.textTertiary,
                                    modifier = Modifier.padding(top = 3.dp),
                                )
                            }
                        }
                    }
                }
            }
        }
    }
}

/** Results scroll inside the panel instead of growing past the keyboard. */
private fun Modifier.heightInPanel(): Modifier = heightIn(max = 320.dp)
