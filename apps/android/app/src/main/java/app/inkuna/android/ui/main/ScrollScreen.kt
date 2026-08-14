package app.inkuna.android.ui.main

import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.inkuna.android.model.PlaceholderBook
import app.inkuna.android.ui.components.BookCover
import app.inkuna.android.ui.theme.InkSpace
import app.inkuna.android.ui.theme.InkTheme
import app.inkuna.android.ui.theme.InkType
import androidx.compose.foundation.clickable
import androidx.compose.ui.res.stringResource
import app.inkuna.android.R

/**
 * Shared scaffold for the four tab screens: vertical scroll, page-margin
 * gutters, 24dp of air below the status bar, 32dp above the tab bar.
 */
@Composable
fun ScrollScreen(
    innerPadding: PaddingValues,
    content: @Composable androidx.compose.foundation.layout.ColumnScope.() -> Unit,
) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(innerPadding)
            .verticalScroll(rememberScrollState())
            .padding(
                start = InkSpace.pageMargin,
                end = InkSpace.pageMargin,
                top = InkSpace.s6,
                bottom = InkSpace.s8,
            ),
        content = content,
    )
}

/** Uppercase eyebrow label with caps tracking — locale-aware uppercasing. */
@Composable
fun EyebrowText(text: String, modifier: Modifier = Modifier) {
    Text(
        text.uppercase(java.util.Locale.getDefault()),
        style = InkType.eyebrow,
        color = InkTheme.colors.textSecondary,
        modifier = modifier,
    )
}

@Composable
fun DisplayTitle(text: String, modifier: Modifier = Modifier) {
    Text(text, style = InkType.display, color = InkTheme.colors.textDisplay, modifier = modifier)
}

@Composable
fun SectionTitle(text: String, modifier: Modifier = Modifier) {
    Text(text, style = InkType.sectionTitle, color = InkTheme.colors.textDisplay, modifier = modifier)
}

/** Centered quiet empty state with 48dp of vertical breathing room. */
@Composable
fun EmptyState(text: String, modifier: Modifier = Modifier) {
    Text(
        text,
        style = InkType.reading,
        color = InkTheme.colors.textTertiary,
        textAlign = TextAlign.Center,
        modifier = modifier
            .fillMaxWidth()
            .padding(vertical = InkSpace.s12),
    )
}

/** Horizontal cover shelf; shadows aren't clipped because nothing clips. */
@Composable
fun ShelfRow(
    books: List<PlaceholderBook>,
    onOpenBook: (PlaceholderBook) -> Unit,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .horizontalScroll(rememberScrollState()),
        horizontalArrangement = Arrangement.spacedBy(InkSpace.s4),
    ) {
        books.forEach { book ->
            ShelfBookView(book, onClick = { onOpenBook(book) })
        }
    }
}

@Composable
fun ShelfBookView(book: PlaceholderBook, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val ink = InkTheme.colors
    val rowLabel = stringResource(R.string.a11y_book_row, book.title, book.author)
    Column(
        modifier = modifier
            .width(104.dp)
            .clickable(onClick = onClick)
            .clearAndSetSemantics {
                contentDescription = rowLabel
                role = Role.Button
            },
    ) {
        BookCover(title = book.title, author = book.author, width = 104.dp, seed = book.coverSeed)
        Text(
            book.title,
            style = InkType.heading.copy(fontSize = 14.sp, lineHeight = 18.sp),
            color = ink.textDisplay,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.padding(top = 9.dp),
        )
        Text(
            book.author,
            style = InkType.caption,
            color = ink.textTertiary,
            modifier = Modifier.padding(top = 2.dp),
        )
    }
}
