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
import app.inkuna.android.model.coverSeed
import app.inkuna.core.Publication as CorePublication
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
import kotlin.math.roundToInt

@Composable
fun TonightScreen(
    innerPadding: PaddingValues,
    onOpenBook: (PlaceholderBook) -> Unit,
    continueReading: CorePublication?,
    onOpenReader: (CorePublication) -> Unit,
) {
    // TODO(core): chips become real collection filters once collections land.
    var selectedChip by rememberSaveable { mutableStateOf(0) }

    ScrollScreen(innerPadding) {
        EyebrowText(stringResource(R.string.tonight_eyebrow))
        Spacer(Modifier.height(6.dp))
        DisplayTitle(stringResource(R.string.tonight_title))
        Spacer(Modifier.height(28.dp))
        HeroCard(
            placeholder = PlaceholderLibrary.heroBook,
            onOpenBook = onOpenBook,
            continueReading = continueReading,
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

/**
 * The card shows the book it opens: whatever the core hands back as the
 * most recently read publication, down to its cover colour. [placeholder]
 * is the design's stand-in book, rendered only while the library is empty
 * and there is nothing to continue.
 */
@Composable
private fun HeroCard(
    placeholder: PlaceholderBook,
    onOpenBook: (PlaceholderBook) -> Unit,
    continueReading: CorePublication?,
    onOpenReader: (CorePublication) -> Unit,
) {
    val ink = InkTheme.colors
    val unknownAuthor = stringResource(R.string.unknown_author)
    val title = continueReading?.title ?: placeholder.title
    val author = if (continueReading != null) {
        continueReading.authors.joinToString(", ").ifEmpty { unknownAuthor }
    } else {
        placeholder.author
    }
    // Rounded like the library rows and iOS, so one book never reads 1% here
    // and 2% a tab away.
    val progress = continueReading
        ?.let { (it.progression * 100).roundToInt().coerceIn(0, 100) }
        ?: placeholder.progress
    val seed = continueReading?.let { coverSeed(it.id) } ?: placeholder.coverSeed
    // TODO(core): "pages left in this chapter" needs a chapter's position
    // range, which the core does not expose yet. Real progress beats an
    // invented page count, so the fraction stands in for it.
    val caption = if (continueReading != null) {
        stringResource(R.string.tonight_percent_read, progress)
    } else {
        stringResource(R.string.tonight_pages_left)
    }
    val open: () -> Unit = if (continueReading != null) {
        { onOpenReader(continueReading) }
    } else {
        { onOpenBook(placeholder) }
    }
    val titleLabel = stringResource(R.string.a11y_book_row, title, author)
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
            title = title,
            author = author,
            width = 96.dp,
            seed = seed,
            modifier = Modifier.clickable(onClick = open),
        )
        Column(Modifier.weight(1f).padding(bottom = 4.dp)) {
            // The card is deliberately not one accessibility element — the
            // inner button must stay reachable; the title carries the
            // detail affordance instead.
            Text(
                title,
                style = InkType.heading,
                color = ink.textDisplay,
                modifier = Modifier
                    .clickable(onClick = open)
                    .semantics {
                        contentDescription = titleLabel
                        role = Role.Button
                    },
            )
            Text(
                author,
                style = InkType.label.copy(fontWeight = FontWeight.Normal),
                color = ink.textSecondary,
                modifier = Modifier.padding(top = 3.dp),
            )
            Spacer(Modifier.height(14.dp))
            InkProgressBar(progress, Modifier.fillMaxWidth())
            Spacer(Modifier.height(InkSpace.s2))
            Text(
                caption,
                style = InkType.caption,
                color = ink.textTertiary,
            )
            Spacer(Modifier.height(14.dp))
            InkButton(
                text = stringResource(R.string.tonight_keep_reading),
                // Reading needs a real book, and the library can still be
                // empty — a fresh install, or every book removed again.
                onClick = { continueReading?.let(onOpenReader) },
                size = InkButtonSize.Small,
                icon = Icons.Outlined.AutoStories,
                enabled = continueReading != null,
            )
        }
    }
}
