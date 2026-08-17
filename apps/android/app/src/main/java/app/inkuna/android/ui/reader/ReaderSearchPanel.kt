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
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
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
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.inkuna.android.R
import app.inkuna.android.ui.components.glassModifier
import app.inkuna.android.ui.stats.hairlineThickness
import app.inkuna.android.ui.theme.InkRadius
import app.inkuna.android.ui.theme.InkTheme
import app.inkuna.android.ui.theme.InkType
import kotlinx.coroutines.delay

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
 * The core's leading context can run long enough that a two-line
 * tail-truncating text pushes the match itself off screen. Keep only the
 * tail of the pre-text — enough to read into the match, never enough to
 * hide it. Shared with the Search tab's excerpts.
 */
internal fun clampedLeadingContext(pre: String): String {
    val budget = 16
    return if (pre.length > budget) "…" + pre.takeLast(budget) else pre
}

/**
 * A beat of quiet before the core is asked: a fast typist's keystrokes
 * cancel their predecessors rather than queueing a query each.
 */
private const val SEARCH_DEBOUNCE_MS = 250L

/**
 * In-book search over the core's case-folded, CJK-aware index. Every hit
 * carries a real place in the book, so tapping one moves the navigator
 * there and leaves the panel behind.
 */
@Composable
fun ReaderSearchPanel(
    topPadding: Dp,
    viewModel: ReaderViewModel,
    onSelect: (ReaderViewModel.SearchHit) -> Unit,
    onClose: () -> Unit,
) {
    val ink = InkTheme.colors
    var query by rememberSaveable { mutableStateOf("") }
    var outcome by remember { mutableStateOf(ReaderViewModel.SearchOutcome()) }
    val focusRequester = remember { FocusRequester() }
    val placeholder = stringResource(R.string.reader_search_placeholder)

    LaunchedEffect(Unit) { focusRequester.requestFocus() }

    // One effect per query: a new keystroke cancels the previous one where
    // it stands — inside the debounce, or mid-flight in the core.
    LaunchedEffect(query) {
        val trimmed = query.trim()
        if (!isSearchable(trimmed)) {
            outcome = ReaderViewModel.SearchOutcome()
            return@LaunchedEffect
        }
        delay(SEARCH_DEBOUNCE_MS)
        outcome = viewModel.search(trimmed)
    }

    val focusManager = LocalFocusManager.current
    Box(
        Modifier
            .fillMaxWidth()
            // The bottom margin keeps the grown panel off the screen edge
            // when no keyboard is up; `imePadding` takes over when it is.
            .padding(start = 14.dp, end = 14.dp, top = topPadding, bottom = 14.dp)
            .navigationBarsPadding()
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
                    cursorBrush = SolidColor(ink.accentText),
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
                // The core's total, not the capped list: a reader told
                // "200 results" when there are 900 has been misinformed —
                // and when the list is capped, say so ("200 of 900").
                val countLabel = when {
                    outcome.hits.isEmpty() -> stringResource(R.string.a11y_no_results)
                    outcome.total > outcome.hits.size ->
                        stringResource(R.string.a11y_result_count_capped, outcome.hits.size, outcome.total)
                    else ->
                        pluralStringResource(R.plurals.a11y_result_count, outcome.total, outcome.total)
                }
                if (outcome.hits.isEmpty()) {
                    Text(
                        stringResource(
                            if (outcome.unavailable) {
                                R.string.reader_search_unavailable
                            } else {
                                R.string.reader_search_empty
                            }
                        ),
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
                            // Grows with results toward the keyboard or
                            // screen edge, then scrolls — matching iOS.
                            .weight(1f, fill = false)
                            .semantics {
                                liveRegion = LiveRegionMode.Polite
                                contentDescription = countLabel
                            },
                    ) {
                        items(
                            outcome.hits,
                            key = { hit -> "${hit.spineIndex}:${hit.charOffset}" },
                        ) { hit ->
                            SearchResultRow(hit = hit, onClick = { onSelect(hit) })
                        }
                    }
                }
            }
        }
    }
}

/**
 * One hit: the snippet with the match lit in the accent, and the honest
 * synthetic page it sits on — omitted rather than guessed when unknown.
 */
@Composable
private fun SearchResultRow(hit: ReaderViewModel.SearchHit, onClick: () -> Unit) {
    val ink = InkTheme.colors
    // `snippetPre` / `snippetPost` already carry their own ellipses.
    val snippet = remember(hit, ink.accentText) {
        buildAnnotatedString {
            append(clampedLeadingContext(hit.snippetPre))
            withStyle(SpanStyle(color = ink.accentText, fontWeight = FontWeight.SemiBold)) {
                append(hit.snippetMatch)
            }
            append(hit.snippetPost)
        }
    }
    Column(
        Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(horizontal = 4.dp, vertical = 11.dp)
    ) {
        Text(
            snippet,
            style = InkType.reading.copy(fontSize = 15.sp, lineHeight = 22.sp),
            color = ink.textDisplay,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
        hit.position?.let { position ->
            Text(
                stringResource(R.string.reader_chapter_page, position),
                style = InkType.caption,
                color = ink.textTertiary,
                modifier = Modifier.padding(top = 3.dp),
            )
        }
    }
}
