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
import androidx.compose.runtime.getValue
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
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.inkuna.android.R
import app.inkuna.android.ui.components.BookCover
import app.inkuna.android.ui.components.InkButton
import app.inkuna.android.ui.components.InkIconButton
import app.inkuna.android.ui.components.InkProgressBar
import app.inkuna.android.ui.main.EmptyState
import app.inkuna.android.ui.main.SectionTitle
import app.inkuna.android.ui.stats.hairlineThickness
import app.inkuna.android.ui.theme.InkSpace
import app.inkuna.android.ui.theme.InkTheme
import app.inkuna.android.ui.theme.InkType

/**
 * Book detail: the cover held at arm's length, progress, and the core's
 * table of contents with the saved position's chapter inked in accent.
 * A chapter row opens the reader at that chapter; the button resumes the
 * saved position.
 */
@Composable
fun BookDetailScreen(
    publicationId: String,
    onBack: () -> Unit,
    onRead: (chapterHref: String?) -> Unit,
    model: BookDetailViewModel = viewModel(
        key = "detail-$publicationId",
        factory = BookDetailViewModel.factory(publicationId),
    ),
) {
    val ink = InkTheme.colors
    val state by model.state.collectAsStateWithLifecycle()

    LifecycleResumeEffect(Unit) {
        model.reload()
        onPauseOrDispose {}
    }

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

        val book = state.book
        if (book == null) {
            if (state.failed) {
                EmptyState(stringResource(R.string.library_unopenable))
            }
            // Still fetching: the screen holds quiet instead of flashing
            // placeholder metadata that names the wrong book.
            return@Column
        }

        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            modifier = Modifier.fillMaxWidth(),
        ) {
            BookCover(
                title = book.title,
                author = book.author,
                width = 150.dp,
                seed = book.seed,
                coverPath = book.coverPath,
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
            val progress = book.progress ?: 0
            InkProgressBar(progress, Modifier.width(200.dp))
            // The honest position line, mirroring the reader: "p. N of M"
            // only when the stored locator carries a synthetic position and
            // the core knows the count — never a fictional page number.
            val position = state.position
            val positionCount = state.positionCount
            Text(
                if (position != null && positionCount != null && positionCount > 0) {
                    stringResource(R.string.reader_page_info, position, positionCount, progress)
                } else {
                    stringResource(R.string.reader_percent, progress)
                },
                style = InkType.caption,
                color = ink.textTertiary,
                modifier = Modifier.padding(top = InkSpace.s2),
            )
            Spacer(Modifier.height(18.dp))
            InkButton(
                text = stringResource(R.string.tonight_keep_reading),
                onClick = { onRead(null) },
                icon = Icons.Outlined.AutoStories,
            )
        }
        Spacer(Modifier.height(InkSpace.s10))
        SectionTitle(stringResource(R.string.detail_contents))
        Spacer(Modifier.height(InkSpace.s2))
        if (state.chapters.isEmpty()) {
            EmptyState(stringResource(R.string.detail_no_contents))
        } else {
            state.chapters.forEachIndexed { index, chapter ->
                ChapterRow(
                    chapter = chapter,
                    current = index == state.currentChapterIndex,
                    onClick = { onRead(chapter.href) },
                )
            }
        }
    }
}

@Composable
private fun ChapterRow(
    chapter: BookDetailViewModel.DetailChapter,
    current: Boolean,
    onClick: () -> Unit,
) {
    val ink = InkTheme.colors
    val label = stringResource(R.string.a11y_chapter_row_no_page, chapter.numeral, chapter.title)
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
                // Nested TOC entries step in with their depth.
                .padding(
                    start = 2.dp + (chapter.depth.coerceAtMost(4) * 14).dp,
                    end = 2.dp,
                    top = 13.dp,
                    bottom = 13.dp,
                ),
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
        }
        Box(
            Modifier
                .fillMaxWidth()
                .height(hairlineThickness())
                .background(ink.borderHairline)
        )
    }
}
