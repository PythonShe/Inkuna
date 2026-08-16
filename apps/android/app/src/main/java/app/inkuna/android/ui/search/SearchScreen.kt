package app.inkuna.android.ui.search

import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
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
import app.inkuna.android.ui.theme.InkSpace

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
            state.query.isBlank() -> if (state.discover.isNotEmpty()) {
                // An empty library has nothing to discover; the quiet
                // screen leaves the search field standing alone.
                EyebrowText(
                    stringResource(R.string.search_recently_added),
                    modifier = Modifier.padding(top = InkSpace.s6, bottom = InkSpace.s4),
                )
                ShelfRow(books = state.discover, onOpenBook = onOpenBook)
            } else {
                Unit
            }
            state.searched && state.results.isEmpty() ->
                EmptyState(stringResource(R.string.library_empty_query))
            else -> {
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
            }
        }
    }
}
