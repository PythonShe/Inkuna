package app.inkuna.android.ui.library

import androidx.annotation.StringRes
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.inkuna.android.R
import app.inkuna.android.ui.components.BookListRow
import app.inkuna.android.ui.components.InkSearchField
import app.inkuna.android.ui.components.InkSegmentedControl
import app.inkuna.android.ui.importing.ImportBooks
import app.inkuna.android.ui.main.DisplayTitle
import app.inkuna.android.ui.main.EmptyState
import app.inkuna.android.ui.main.ScrollScreen
import app.inkuna.android.ui.theme.InkSpace

private val SEGMENT_LABELS: Map<LibrarySegment, Int> = mapOf(
    LibrarySegment.Reading to R.string.library_seg_reading,
    LibrarySegment.Finished to R.string.library_seg_finished,
    LibrarySegment.Wishlist to R.string.library_seg_wishlist,
)

@StringRes
private fun emptyMessage(emptiness: LibraryEmptiness): Int = when (emptiness) {
    is LibraryEmptiness.WholeLibrary -> R.string.library_empty_all
    is LibraryEmptiness.Shelf -> when (emptiness.kind) {
        LibraryEmptiness.Shelf.Kind.Finished -> R.string.library_empty_finished
        LibraryEmptiness.Shelf.Kind.Wishlist -> R.string.library_empty_wishlist
        LibraryEmptiness.Shelf.Kind.NoMatches -> R.string.library_empty_query
        LibraryEmptiness.Shelf.Kind.Unopenable -> R.string.library_unopenable
        LibraryEmptiness.Shelf.Kind.Reading -> R.string.library_empty_reading
    }
}

/**
 * The Library tab: search, the Reading/Finished/Wishlist segments, and the
 * book list — all of it rendered from the Rust core.
 *
 * TODO(core): the Wishlist segment has no core shelf yet (file-less
 * publications are deferred to their own spec), so it stays visible and
 * empty.
 */
@Composable
fun LibraryScreen(
    innerPadding: PaddingValues,
    onOpenBook: (String) -> Unit,
    model: LibraryViewModel = viewModel(),
) {
    val state by model.state.collectAsStateWithLifecycle()

    val segments = LibrarySegment.entries
    // Addressed by index: two shelves may well share a translation, and a
    // label round-trip would then pick the wrong one.
    val segmentLabels = segments.map { stringResource(SEGMENT_LABELS.getValue(it)) }

    ScrollScreen(innerPadding) {
        DisplayTitle(stringResource(R.string.library_title))
        Spacer(Modifier.height(InkSpace.s5))
        InkSearchField(
            value = state.query,
            onValueChange = model::setQuery,
            placeholder = stringResource(R.string.library_search_placeholder),
        )
        Spacer(Modifier.height(InkSpace.s4))
        InkSegmentedControl(
            options = segmentLabels,
            selectedIndex = segments.indexOf(state.segment),
            onSelect = { index -> model.setSegment(segments[index]) },
            modifier = Modifier.fillMaxWidth(),
        )
        Spacer(Modifier.height(InkSpace.s2))
        // An empty library gets the invitation to import; otherwise this is
        // the plain "Add books" affordance. Either way it refreshes the list
        // itself, so an import from any source lands without polling.
        ImportBooks(
            libraryIsEmpty = state.rows.isEmpty() && state.emptiness is LibraryEmptiness.WholeLibrary,
            onLibraryChanged = model::reload,
        )
        Spacer(Modifier.height(InkSpace.s2))
        if (state.rows.isEmpty()) {
            // The whole-library case is already spoken for by the import
            // invitation above; repeating it as an empty state would say the
            // same thing twice.
            if (state.emptiness !is LibraryEmptiness.WholeLibrary) {
                EmptyState(stringResource(emptyMessage(state.emptiness)))
            }
        } else {
            state.rows.forEach { row ->
                BookListRow(
                    title = row.title,
                    author = row.author,
                    progress = row.progress,
                    seed = row.seed,
                    // The core owns every book's file, so a listed book is
                    // always on disk — no cloud-only state to badge.
                    downloaded = true,
                    onClick = { onOpenBook(row.id) },
                )
            }
        }
    }
}
