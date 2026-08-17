package app.inkuna.android.ui.search

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.inkuna.android.R
import app.inkuna.android.ui.components.BookListRow
import app.inkuna.android.ui.components.InkSearchField
import app.inkuna.android.ui.main.DisplayTitle
import app.inkuna.android.ui.main.EmptyState
import app.inkuna.android.ui.main.EyebrowText
import app.inkuna.android.ui.main.ScrollScreen
import app.inkuna.android.ui.main.ShelfRow
import app.inkuna.android.ui.reader.clampedLeadingContext
import app.inkuna.android.ui.theme.InkSpace
import app.inkuna.android.ui.theme.InkTheme
import app.inkuna.android.ui.theme.InkType

/**
 * The Search tab: library search through the core — case-folded, CJK-safe
 * matching over titles and authors — with a "New this week" shelf of the
 * freshest imports while the query is empty.
 */
@Composable
fun SearchScreen(
    innerPadding: PaddingValues,
    onOpenBook: (String) -> Unit,
    model: SearchViewModel = viewModel(),
) {
    val state by model.state.collectAsStateWithLifecycle()

    // Progress moves while the reader is open, and imports land from other
    // tabs; result rows behind it are stale by the time it is popped.
    LifecycleResumeEffect(Unit) {
        model.reload()
        onPauseOrDispose {}
    }

    ScrollScreen(innerPadding) {
        DisplayTitle(stringResource(R.string.search_title))
        Spacer(Modifier.height(InkSpace.s5))
        InkSearchField(
            value = state.query,
            onValueChange = model::setQuery,
            placeholder = stringResource(R.string.search_placeholder),
        )
        when {
            state.failed -> EmptyState(stringResource(R.string.library_unopenable))
            state.query.isBlank() -> {
                // An empty library has nothing to discover; the quiet
                // screen leaves the search field standing alone.
                if (state.discover.isNotEmpty()) {
                    EyebrowText(
                        stringResource(R.string.search_recently_added),
                        modifier = Modifier.padding(top = InkSpace.s6, bottom = InkSpace.s4),
                    )
                    ShelfRow(books = state.discover, onOpenBook = onOpenBook)
                }
            }
            state.searched && state.results.isEmpty() && state.fullText.isEmpty() ->
                EmptyState(stringResource(R.string.library_empty_query))
            state.searched -> {
                Spacer(Modifier.height(InkSpace.s3))
                state.results.forEach { row ->
                    BookListRow(
                        title = row.title,
                        author = row.author,
                        progress = row.progress,
                        seed = row.seed,
                        coverPath = row.coverPath,
                        // The core owns every book's file, so a listed book
                        // is always on disk — no cloud-only state to badge.
                        downloaded = true,
                        onClick = { onOpenBook(row.id) },
                    )
                }
                // Books whose *text* matches, under the ones whose covers do.
                if (state.fullText.isNotEmpty()) {
                    EyebrowText(
                        stringResource(R.string.search_in_books),
                        modifier = Modifier.padding(top = InkSpace.s6, bottom = InkSpace.s2),
                    )
                    state.fullText.forEach { hit ->
                        FullTextResult(hit = hit, onOpenBook = onOpenBook)
                    }
                }
            }
            else -> Unit
        }
    }
}

/**
 * One book whose text matches: the same row every other shelf uses, with
 * the core's excerpt under it and the matched run lit in the accent. The
 * excerpt opens the book exactly as the row does — one target, two halves.
 */
@Composable
private fun FullTextResult(
    hit: SearchViewModel.FullTextRow,
    onOpenBook: (String) -> Unit,
) {
    val ink = InkTheme.colors
    val open = { onOpenBook(hit.book.id) }
    val excerpt = remember(hit, ink.accentText) {
        buildAnnotatedString {
            // Head-clamped so the two-line tail truncation can never push
            // the match itself out of the excerpt.
            append(clampedLeadingContext(hit.excerptPre))
            withStyle(SpanStyle(color = ink.accentText, fontWeight = FontWeight.SemiBold)) {
                append(hit.excerptMatch)
            }
            append(hit.excerptPost)
        }
    }
    Column {
        BookListRow(
            title = hit.book.title,
            author = hit.book.author,
            progress = hit.book.progress,
            seed = hit.book.seed,
            coverPath = hit.book.coverPath,
            downloaded = true,
            onClick = open,
        )
        Text(
            excerpt,
            style = InkType.reading,
            color = ink.textSecondary,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier
                .clickable(onClick = open)
                .padding(bottom = InkSpace.s3),
        )
    }
}
