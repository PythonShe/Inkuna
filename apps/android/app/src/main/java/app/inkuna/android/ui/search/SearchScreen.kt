package app.inkuna.android.ui.search

import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
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
import app.inkuna.android.ui.main.DisplayTitle
import app.inkuna.android.ui.main.EmptyState
import app.inkuna.android.ui.main.EyebrowText
import app.inkuna.android.ui.main.ScrollScreen
import app.inkuna.android.ui.main.ShelfRow
import app.inkuna.android.ui.theme.InkSpace

@Composable
fun SearchScreen(
    innerPadding: PaddingValues,
    onOpenBook: (PlaceholderBook) -> Unit,
) {
    var query by rememberSaveable { mutableStateOf("") }
    val trimmed = query.trim().lowercase()
    // TODO(core): route through the core's CJK-aware search index.
    val results = if (trimmed.isEmpty()) emptyList() else PlaceholderLibrary.books.filter {
        "${it.title} ${it.author}".lowercase().contains(trimmed)
    }

    ScrollScreen(innerPadding) {
        DisplayTitle(stringResource(R.string.search_title))
        Spacer(Modifier.height(InkSpace.s5))
        InkSearchField(
            value = query,
            onValueChange = { query = it },
            placeholder = stringResource(R.string.search_placeholder),
        )
        when {
            trimmed.isEmpty() -> {
                EyebrowText(
                    stringResource(R.string.search_new_this_week),
                    modifier = Modifier.padding(top = InkSpace.s6, bottom = InkSpace.s4),
                )
                ShelfRow(books = PlaceholderLibrary.discover, onOpenBook = onOpenBook)
            }
            results.isEmpty() -> EmptyState(stringResource(R.string.library_empty_query))
            else -> {
                Spacer(Modifier.height(InkSpace.s3))
                results.forEach { book ->
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
}
