package app.inkuna.android.ui.tonight

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.AutoStories
import androidx.compose.material.icons.outlined.Person
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.inkuna.android.R
import app.inkuna.android.model.BookRow
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
import app.inkuna.android.ui.settings.SettingsSheet
import app.inkuna.android.ui.theme.InkRadius
import app.inkuna.android.ui.theme.InkSpace
import app.inkuna.android.ui.theme.InkTheme
import app.inkuna.android.ui.theme.InkType

@Composable
fun TonightScreen(
    innerPadding: PaddingValues,
    onOpenBook: (String) -> Unit,
    onOpenReader: (String) -> Unit,
    model: TonightViewModel = viewModel(),
) {
    val state by model.state.collectAsStateWithLifecycle()

    // Progress moves while the reader is open, and imports land from other
    // tabs; coming back must not show last week's hero.
    LifecycleResumeEffect(Unit) {
        model.reload()
        onPauseOrDispose {}
    }

    // TODO(core): chips become real collection filters once collections land.
    var selectedChip by rememberSaveable { mutableStateOf(0) }

    // The account affordance lives beside the header — this screen has no
    // top app bar to put it in; the sheet hosts the design's Account page.
    var showSettings by rememberSaveable { mutableStateOf(false) }
    if (showSettings) {
        SettingsSheet(onDismiss = { showSettings = false })
    }

    ScrollScreen(innerPadding) {
        Row(verticalAlignment = Alignment.Top) {
            Column(Modifier.weight(1f)) {
                EyebrowText(stringResource(R.string.tonight_eyebrow))
                Spacer(Modifier.height(6.dp))
                DisplayTitle(stringResource(R.string.tonight_title))
            }
            AccountAvatarButton(onClick = { showSettings = true })
        }
        Spacer(Modifier.height(28.dp))
        HeroCard(
            continueReading = state.continueReading,
            pagesLeftInChapter = state.pagesLeftInChapter,
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
        // The nightstand shows only when there is something on it — an
        // empty core shelf removes the section instead of rendering
        // stand-ins.
        if (state.nightstand.isNotEmpty()) {
            Spacer(Modifier.height(InkSpace.s4))
            SectionTitle(stringResource(R.string.tonight_nightstand))
            Spacer(Modifier.height(InkSpace.s4))
            ShelfRow(books = state.nightstand, onOpenBook = onOpenBook)
        }
    }
}

/**
 * The header's account affordance: a 40dp avatar disc top-aligned with
 * the eyebrow/title block, sized identically on iOS. Recessed rather
 * than the design's surface+shadow: that treatment suited a photo
 * avatar, while a glyph on bright surface read as a white coin — this
 * matches the Account sheet's own avatar idiom.
 */
@Composable
private fun AccountAvatarButton(onClick: () -> Unit) {
    val ink = InkTheme.colors
    Box(
        Modifier
            .size(40.dp)
            .clip(CircleShape)
            .background(ink.bgRecessed)
            .clickable(onClick = onClick, role = Role.Button),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            Icons.Outlined.Person,
            contentDescription = stringResource(R.string.settings_title),
            tint = ink.textSecondary,
            modifier = Modifier.size(20.dp),
        )
    }
}

/**
 * The card shows the book it opens: whatever the core hands back as the
 * most recently read publication, down to its cover. With nothing to
 * continue it renders the design's stand-in book — inert scenery, never a
 * destination.
 */
@Composable
private fun HeroCard(
    continueReading: BookRow?,
    pagesLeftInChapter: Int?,
    onOpenReader: (String) -> Unit,
) {
    val ink = InkTheme.colors
    val placeholder = PlaceholderLibrary.heroBook
    val title = continueReading?.title ?: placeholder.title
    val author = continueReading?.author ?: placeholder.author
    val progress = continueReading?.let { it.progress ?: 0 } ?: placeholder.progress
    val seed = continueReading?.seed ?: placeholder.coverSeed
    // The chapter's remaining synthetic positions when the core knows them
    // — it learns a book's chapter ranges the first time the reader opens
    // it — and the honest book-wide fraction until then.
    val caption = when {
        continueReading == null -> stringResource(R.string.tonight_pages_left)
        pagesLeftInChapter != null -> pluralStringResource(
            R.plurals.tonight_pages_left_chapter,
            pagesLeftInChapter,
            pagesLeftInChapter,
        )
        else -> stringResource(R.string.tonight_percent_read, progress)
    }
    val open: (() -> Unit)? = continueReading?.let { book -> { onOpenReader(book.id) } }
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
            coverPath = continueReading?.coverPath,
            modifier = if (open != null) Modifier.clickable(onClick = open) else Modifier,
        )
        Column(Modifier.weight(1f).padding(bottom = 4.dp)) {
            // The card is deliberately not one accessibility element — the
            // inner button must stay reachable; the title carries the
            // detail affordance instead.
            Text(
                title,
                style = InkType.heading,
                color = ink.textDisplay,
                modifier = if (open != null) {
                    Modifier
                        .clickable(onClick = open)
                        .semantics {
                            contentDescription = titleLabel
                            role = Role.Button
                        }
                } else {
                    Modifier.semantics { contentDescription = titleLabel }
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
                onClick = { open?.invoke() },
                size = InkButtonSize.Small,
                icon = Icons.Outlined.AutoStories,
                enabled = open != null,
            )
        }
    }
}
