package app.inkuna.android.ui.library

import androidx.annotation.StringRes
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import app.inkuna.android.R
import app.inkuna.android.model.PlaceholderBook
import app.inkuna.android.model.PlaceholderLibrary
import app.inkuna.android.ui.components.BookListRow
import app.inkuna.android.ui.components.InkSearchField
import app.inkuna.android.ui.components.InkSegmentedControl
import app.inkuna.android.ui.main.DisplayTitle
import app.inkuna.android.ui.main.EmptyState
import app.inkuna.android.ui.main.ScrollScreen
import app.inkuna.android.ui.theme.InkSpace

private enum class LibrarySegment(@param:StringRes val labelRes: Int) {
    Reading(R.string.library_seg_reading),
    Finished(R.string.library_seg_finished),
    Wishlist(R.string.library_seg_wishlist),
}

@Composable
fun LibraryScreen(
    innerPadding: PaddingValues,
    onOpenBook: (PlaceholderBook) -> Unit,
) {
    var query by rememberSaveable { mutableStateOf("") }
    var segment by rememberSaveable { mutableStateOf(LibrarySegment.Reading) }

    val segments = LibrarySegment.entries
    // Addressed by index: two shelves may well share a translation, and a
    // label round-trip would then pick the wrong one.
    val segmentLabels = segments.map { stringResource(it.labelRes) }

    // TODO(core): Finished/Wishlist shelves don't exist yet; only Reading
    // has placeholder rows, matching the prototype.
    val trimmed = query.trim().lowercase()
    val books = when (segment) {
        LibrarySegment.Reading -> PlaceholderLibrary.books.filter {
            trimmed.isEmpty() || "${it.title} ${it.author}".lowercase().contains(trimmed)
        }
        else -> emptyList()
    }

    ScrollScreen(innerPadding) {
        DisplayTitle(stringResource(R.string.library_title))
        Spacer(Modifier.height(InkSpace.s5))
        InkSearchField(
            value = query,
            onValueChange = { query = it },
            placeholder = stringResource(R.string.library_search_placeholder),
        )
        Spacer(Modifier.height(InkSpace.s4))
        InkSegmentedControl(
            options = segmentLabels,
            selectedIndex = segments.indexOf(segment),
            onSelect = { index -> segment = segments[index] },
            modifier = Modifier.fillMaxWidth(),
        )
        Spacer(Modifier.height(InkSpace.s2))
        if (books.isEmpty()) {
            EmptyState(
                when {
                    segment == LibrarySegment.Finished -> stringResource(R.string.library_empty_finished)
                    segment == LibrarySegment.Wishlist -> stringResource(R.string.library_empty_wishlist)
                    else -> stringResource(R.string.library_empty_query)
                }
            )
        } else {
            books.forEach { book ->
                BookListRow(
                    title = book.title,
                    author = book.author,
                    progress = book.progress,
                    seed = book.coverSeed,
                    downloaded = book.downloaded,
                    onClick = { onOpenBook(book) },
                )
            }
        }
    }
}
