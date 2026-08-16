package app.inkuna.android.ui.detail

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowBack
import androidx.compose.material.icons.outlined.AutoStories
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.inkuna.android.R
import app.inkuna.android.model.PlaceholderBook
import app.inkuna.android.model.PlaceholderChapter
import app.inkuna.android.model.PlaceholderLibrary
import app.inkuna.core.Publication as CorePublication
import app.inkuna.android.ui.components.BookCover
import app.inkuna.android.ui.components.InkButton
import app.inkuna.android.ui.components.InkIconButton
import app.inkuna.android.ui.components.InkProgressBar
import app.inkuna.android.ui.main.SectionTitle
import app.inkuna.android.ui.stats.hairlineThickness
import app.inkuna.android.ui.theme.InkSpace
import app.inkuna.android.ui.theme.InkTheme
import app.inkuna.android.ui.theme.InkType

// TODO(core): the detail metadata, progress, and contents list still render
// the placeholder book; only the read affordances are core-backed. The
// library wiring workstream replaces the visuals with `publication`.
@Composable
fun BookDetailScreen(
    book: PlaceholderBook,
    publication: CorePublication?,
    onBack: () -> Unit,
    onRead: (CorePublication) -> Unit,
) {
    val ink = InkTheme.colors
    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(ink.bgApp)
            .safeDrawingPadding()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = InkSpace.pageMargin)
            .padding(top = InkSpace.s3, bottom = InkSpace.s16),
    ) {
        InkIconButton(
            icon = Icons.AutoMirrored.Outlined.ArrowBack,
            contentDescription = stringResource(R.string.a11y_back),
            onClick = onBack,
        )
        Spacer(Modifier.height(18.dp))
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            modifier = Modifier.fillMaxWidth(),
        ) {
            BookCover(
                title = book.title,
                author = book.author,
                width = 150.dp,
                seed = book.coverSeed,
            )
            Text(
                book.title,
                style = InkType.displaySmall,
                color = ink.textDisplay,
                textAlign = TextAlign.Center,
                modifier = Modifier.padding(top = 22.dp),
            )
            Text(
                book.author,
                style = InkType.label.copy(fontWeight = FontWeight.Normal),
                color = ink.textSecondary,
                modifier = Modifier.padding(top = 5.dp),
            )
            Spacer(Modifier.height(InkSpace.s5))
            InkProgressBar(book.progress, Modifier.width(200.dp))
            Text(
                stringResource(
                    R.string.reader_page_info,
                    book.currentPage,
                    book.pageCount,
                    book.progress,
                ),
                style = InkType.caption,
                color = ink.textTertiary,
                modifier = Modifier.padding(top = InkSpace.s2),
            )
            Spacer(Modifier.height(18.dp))
            InkButton(
                text = stringResource(R.string.tonight_keep_reading),
                // Reading needs a real book: until an import UI exists the
                // library can be empty, and then there is nothing to open.
                onClick = { publication?.let(onRead) },
                icon = Icons.Outlined.AutoStories,
                enabled = publication != null,
            )
        }
        Spacer(Modifier.height(InkSpace.s10))
        SectionTitle(stringResource(R.string.detail_contents))
        Spacer(Modifier.height(InkSpace.s2))
        PlaceholderLibrary.chapters.forEachIndexed { index, chapter ->
            // TODO(core): jump to the tapped chapter; the placeholder rows
            // always open the current reading position (the in-reader
            // contents sheet does jump for real).
            ChapterRow(
                chapter = chapter,
                current = index == PlaceholderLibrary.currentChapterIndex,
                onClick = { publication?.let(onRead) },
            )
        }
    }
}

@Composable
private fun ChapterRow(
    chapter: PlaceholderChapter,
    current: Boolean,
    onClick: () -> Unit,
) {
    val ink = InkTheme.colors
    val label = stringResource(R.string.a11y_chapter_row, chapter.numeral, chapter.title, chapter.page)
    Column {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(InkSpace.s3),
            modifier = Modifier
                .fillMaxWidth()
                .clickable(onClick = onClick)
                .clearAndSetSemantics {
                    contentDescription = label
                    role = Role.Button
                }
                .padding(horizontal = 2.dp, vertical = 13.dp),
        ) {
            Text(
                chapter.numeral,
                style = InkType.caption,
                color = if (current) ink.accentText else ink.textTertiary,
                modifier = Modifier.widthIn(min = 26.dp),
            )
            Text(
                chapter.title,
                style = InkType.reading.copy(
                    fontSize = 16.sp,
                    lineHeight = 21.sp,
                    fontWeight = if (current) FontWeight.SemiBold else FontWeight.Normal,
                ),
                color = if (current) ink.accentText else ink.textDisplay,
                modifier = Modifier.weight(1f),
            )
            Text(
                stringResource(R.string.reader_chapter_page, chapter.page),
                style = InkType.caption,
                color = ink.textTertiary,
            )
        }
        Box(
            Modifier
                .fillMaxWidth()
                .height(hairlineThickness())
                .background(ink.borderHairline)
        )
    }
}
