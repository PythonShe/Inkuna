package app.inkuna.android.ui.tonight

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.AutoStories
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import app.inkuna.android.R
import app.inkuna.android.model.PlaceholderBook
import app.inkuna.android.model.PlaceholderLibrary
import app.inkuna.android.ui.components.BookCover
import app.inkuna.android.ui.components.InkButton
import app.inkuna.android.ui.components.InkButtonSize
import app.inkuna.android.ui.components.InkChip
import app.inkuna.android.ui.components.InkProgressBar
import app.inkuna.android.ui.components.inkShadow
import app.inkuna.android.ui.main.DisplayTitle
import app.inkuna.android.ui.main.EyebrowText
import app.inkuna.android.ui.main.ScrollScreen
import app.inkuna.android.ui.main.SectionTitle
import app.inkuna.android.ui.main.ShelfRow
import app.inkuna.android.ui.theme.InkRadius
import app.inkuna.android.ui.theme.InkSpace
import app.inkuna.android.ui.theme.InkTheme
import app.inkuna.android.ui.theme.InkType

@Composable
fun TonightScreen(
    innerPadding: PaddingValues,
    onOpenBook: (PlaceholderBook) -> Unit,
    onOpenReader: (PlaceholderBook) -> Unit,
) {
    // TODO(core): chips become real collection filters once collections land.
    var selectedChip by rememberSaveable { mutableStateOf(0) }

    ScrollScreen(innerPadding) {
        EyebrowText(stringResource(R.string.tonight_eyebrow))
        Spacer(Modifier.height(6.dp))
        DisplayTitle(stringResource(R.string.tonight_title))
        Spacer(Modifier.height(28.dp))
        HeroCard(
            book = PlaceholderLibrary.heroBook,
            onOpenBook = onOpenBook,
            onOpenReader = onOpenReader,
        )
        Spacer(Modifier.height(InkSpace.s8))
        Row(horizontalArrangement = Arrangement.spacedBy(InkSpace.s2)) {
            PlaceholderLibrary.tonightChips.forEachIndexed { index, labelRes ->
                InkChip(
                    text = stringResource(labelRes),
                    selected = index == selectedChip,
                    onClick = { selectedChip = index },
                )
            }
        }
        Spacer(Modifier.height(InkSpace.s4))
        SectionTitle(stringResource(R.string.tonight_nightstand))
        Spacer(Modifier.height(InkSpace.s4))
        ShelfRow(books = PlaceholderLibrary.shelf, onOpenBook = onOpenBook)
    }
}

@Composable
private fun HeroCard(
    book: PlaceholderBook,
    onOpenBook: (PlaceholderBook) -> Unit,
    onOpenReader: (PlaceholderBook) -> Unit,
) {
    val ink = InkTheme.colors
    val titleLabel = stringResource(R.string.a11y_book_row, book.title, book.author)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .inkShadow(6.dp, InkRadius.xlShape)
            .clip(InkRadius.xlShape)
            .background(ink.bgSurface)
            .padding(InkSpace.s5),
        horizontalArrangement = Arrangement.spacedBy(18.dp),
        verticalAlignment = Alignment.Bottom,
    ) {
        BookCover(
            title = book.title,
            author = book.author,
            width = 96.dp,
            seed = book.coverSeed,
            modifier = Modifier.clickable { onOpenBook(book) },
        )
        Column(Modifier.weight(1f).padding(bottom = 4.dp)) {
            // The card is deliberately not one accessibility element — the
            // inner button must stay reachable; the title carries the
            // detail affordance instead.
            Text(
                book.title,
                style = InkType.heading,
                color = ink.textDisplay,
                modifier = Modifier
                    .clickable { onOpenBook(book) }
                    .semantics {
                        contentDescription = titleLabel
                        role = Role.Button
                    },
            )
            Text(
                book.author,
                style = InkType.label.copy(fontWeight = FontWeight.Normal),
                color = ink.textSecondary,
                modifier = Modifier.padding(top = 3.dp),
            )
            Spacer(Modifier.height(14.dp))
            InkProgressBar(book.progress, Modifier.fillMaxWidth())
            Spacer(Modifier.height(InkSpace.s2))
            Text(
                stringResource(R.string.tonight_pages_left),
                style = InkType.caption,
                color = ink.textTertiary,
            )
            Spacer(Modifier.height(14.dp))
            InkButton(
                text = stringResource(R.string.tonight_keep_reading),
                onClick = { onOpenReader(book) },
                size = InkButtonSize.Small,
                icon = Icons.Outlined.AutoStories,
            )
        }
    }
}
