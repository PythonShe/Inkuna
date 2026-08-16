package app.inkuna.android.ui.importing

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.InfiniteRepeatableSpec
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Check
import androidx.compose.material.icons.outlined.ContentCopy
import androidx.compose.material.icons.outlined.ErrorOutline
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.SheetState
import androidx.compose.material3.SheetValue
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import app.inkuna.android.R
import app.inkuna.android.importing.ImportFailure
import app.inkuna.android.importing.ImportFailureKind
import app.inkuna.android.importing.ImportPhase
import app.inkuna.android.importing.ImportReport
import app.inkuna.android.importing.ImportState
import app.inkuna.android.importing.ImportedBook
import app.inkuna.android.ui.components.InkButton
import app.inkuna.android.ui.components.InkButtonVariant
import app.inkuna.android.ui.main.EyebrowText
import app.inkuna.android.ui.theme.InkMotion
import app.inkuna.android.ui.theme.InkRadius
import app.inkuna.android.ui.theme.InkSpace
import app.inkuna.android.ui.theme.InkTheme
import app.inkuna.android.ui.theme.InkType

/**
 * The one surface the import flow speaks through: a bottom sheet that shows
 * progress while a run is live and turns into its report when it ends.
 *
 * It is modal on purpose. An import mutates the shelf the reader is looking
 * at, and the run is short; a sheet that cannot be swiped away mid-copy is
 * both simpler to reason about and impossible to lose behind a scroll.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun ImportSheet(
    state: ImportState,
    onCancel: () -> Unit,
    onDismiss: () -> Unit,
) {
    if (state is ImportState.Idle) return
    val running = state is ImportState.Running
    // Read through a State, and remember the predicate itself, for two
    // reasons that both bit in testing:
    //
    //  * `confirmValueChange` gates *every* value change, the sheet's own
    //    entrance included. A naive `{ !running }` vetoes the move to
    //    Expanded while a run is live, so the progress pane composes,
    //    updates, and is never once drawn — the sheet only appeared after
    //    the run had already finished.
    //  * `rememberModalBottomSheetState` keys its saved state on this
    //    lambda. A lambda that captures `running` is a new instance the
    //    moment the run ends, which throws away the live SheetState and
    //    restarts the entrance from Hidden.
    //
    // So: allow every target except Hidden, and refuse Hidden only while a
    // run is in flight — which is exactly the intent, "not dismissible
    // mid-import", with none of the collateral.
    val runningNow = rememberUpdatedState(running)
    val sheetState: SheetState = rememberModalBottomSheetState(
        skipPartiallyExpanded = true,
        confirmValueChange = remember {
            { target: SheetValue -> target != SheetValue.Hidden || !runningNow.value }
        },
    )
    ModalBottomSheet(
        onDismissRequest = { if (!running) onDismiss() },
        sheetState = sheetState,
        containerColor = InkTheme.colors.bgSurface,
        contentColor = InkTheme.colors.textBody,
        scrimColor = InkTheme.colors.scrim,
        dragHandle = { SheetHandle() },
    ) {
        Column(
            Modifier
                .fillMaxWidth()
                .padding(horizontal = InkSpace.pageMargin)
                .padding(bottom = InkSpace.s6)
                .navigationBarsPadding(),
        ) {
            AnimatedContent(
                // The state itself, never a derived flag: the outgoing pane
                // keeps rendering during the cross-fade, and reading a
                // captured `state` there would hand the progress pane a
                // report (a ClassCastException, seen in testing).
                targetState = state,
                // Only the phase flip is a transition; progress ticks
                // recompose the same pane in place.
                contentKey = { it is ImportState.Finished },
                transitionSpec = {
                    fadeIn(tween(InkMotion.durMed, easing = InkMotion.easeQuiet)) togetherWith
                        fadeOut(tween(InkMotion.durFast, easing = InkMotion.easeQuiet))
                },
                label = "importPane",
            ) { pane ->
                when (pane) {
                    is ImportState.Finished -> ImportReportPane(pane.report, onDismiss)
                    is ImportState.Running -> ImportProgressPane(pane, onCancel)
                    ImportState.Idle -> Unit
                }
            }
        }
    }
}

@Composable
private fun SheetHandle() {
    Box(Modifier.fillMaxWidth().padding(vertical = InkSpace.s3), contentAlignment = Alignment.Center) {
        Box(
            Modifier
                .size(width = 36.dp, height = 4.dp)
                .clip(InkRadius.pillShape)
                .background(InkTheme.colors.borderHairline),
        )
    }
}

@Composable
private fun ImportProgressPane(state: ImportState.Running, onCancel: () -> Unit) {
    val ink = InkTheme.colors
    val phaseLabel = stringResource(
        when (state.phase) {
            ImportPhase.Copying -> R.string.import_phase_copying
            ImportPhase.Reading -> R.string.import_phase_reading
        }
    )
    val a11y = stringResource(R.string.a11y_import_progress, state.completed + 1, state.total)
    Column(Modifier.fillMaxWidth().semantics { liveRegion = LiveRegionMode.Polite }) {
        EyebrowText(stringResource(R.string.import_progress_eyebrow))
        Spacer(Modifier.height(InkSpace.s3))
        Text(
            state.currentName,
            style = InkType.heading,
            color = ink.textDisplay,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.semantics { contentDescription = a11y },
        )
        Spacer(Modifier.height(InkSpace.s4))
        ImportProgressTrack(state.fraction)
        Spacer(Modifier.height(InkSpace.s3))
        Row(
            Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(phaseLabel, style = InkType.caption, color = ink.textTertiary)
            if (state.total > 1) {
                Text(
                    stringResource(R.string.import_progress_count, state.completed + 1, state.total),
                    style = InkType.caption,
                    color = ink.textTertiary,
                )
            }
        }
        Spacer(Modifier.height(InkSpace.s5))
        InkButton(
            text = stringResource(R.string.import_cancel),
            onClick = onCancel,
            variant = InkButtonVariant.Ghost,
        )
    }
}

/**
 * A 3dp track that settles toward [fraction], or sweeps a band across
 * itself when the core is working and reports nothing. The sweep is the
 * honest signal: it says "still going", not "this far along".
 */
@Composable
private fun ImportProgressTrack(fraction: Float?) {
    val ink = InkTheme.colors
    val settled by animateFloatAsState(
        targetValue = fraction ?: 0f,
        animationSpec = tween(InkMotion.durMed, easing = InkMotion.easePage),
        label = "importProgress",
    )
    BoxWithConstraints(
        Modifier
            .fillMaxWidth()
            .height(3.dp)
            .clip(InkRadius.pillShape)
            .background(ink.bgRecessed),
    ) {
        if (fraction != null) {
            Box(
                Modifier
                    .fillMaxHeight()
                    .fillMaxWidth(settled)
                    .clip(InkRadius.pillShape)
                    .background(ink.accent),
            )
        } else {
            val trackWidth = maxWidth
            val bandWidth = trackWidth * 0.34f
            val transition = rememberInfiniteTransition(label = "importSweep")
            val travel = with(LocalDensity.current) { (trackWidth + bandWidth).toPx() }
            val offset by transition.animateFloat(
                initialValue = -with(LocalDensity.current) { bandWidth.toPx() },
                targetValue = travel,
                animationSpec = InfiniteRepeatableSpec(
                    animation = tween(1_400, easing = CubicBezierEasing(0.5f, 0f, 0.5f, 1f)),
                    repeatMode = RepeatMode.Restart,
                ),
                label = "importSweepOffset",
            )
            Box(
                Modifier
                    .offset { IntOffset(offset.toInt(), 0) }
                    .width(bandWidth)
                    .fillMaxHeight()
                    .clip(InkRadius.pillShape)
                    .background(ink.accent),
            )
        }
    }
}

@Composable
private fun ImportReportPane(report: ImportReport, onDismiss: () -> Unit) {
    val ink = InkTheme.colors
    val title = when {
        report.cancelled && report.added.isEmpty() -> stringResource(R.string.import_result_cancelled)
        report.added.isNotEmpty() ->
            pluralStringResource(R.plurals.import_result_added, report.added.size, report.added.size)
        report.duplicates.isNotEmpty() && report.failures.isEmpty() ->
            stringResource(R.string.import_result_already)
        else -> stringResource(R.string.import_result_nothing)
    }
    Column(
        Modifier
            .fillMaxWidth()
            // Long reports scroll inside the sheet rather than pushing the
            // Done button past the bottom edge.
            .verticalScroll(rememberScrollState())
            .semantics { liveRegion = LiveRegionMode.Polite },
    ) {
        Text(title, style = InkType.title, color = ink.textDisplay)
        Spacer(Modifier.height(InkSpace.s4))

        if (report.added.isNotEmpty()) {
            ReportSection(stringResource(R.string.import_section_added)) {
                report.added.forEach { BookRow(it, Icons.Outlined.Check, ink.positive) }
            }
        }
        // Duplicates are named, never swallowed: the reader picked that file
        // on purpose and is owed an answer about it.
        if (report.duplicates.isNotEmpty()) {
            ReportSection(stringResource(R.string.import_section_duplicates)) {
                report.duplicates.forEach { BookRow(it, Icons.Outlined.ContentCopy, ink.textTertiary) }
            }
        }
        if (report.failures.isNotEmpty()) {
            ReportSection(stringResource(R.string.import_section_failures)) {
                report.failures.forEach { FailureRow(it) }
            }
        }
        Spacer(Modifier.height(InkSpace.s5))
        InkButton(text = stringResource(R.string.import_done), onClick = onDismiss)
    }
}

@Composable
private fun ReportSection(title: String, content: @Composable () -> Unit) {
    Column(Modifier.fillMaxWidth().padding(bottom = InkSpace.s4)) {
        EyebrowText(title)
        Spacer(Modifier.height(InkSpace.s2))
        content()
    }
}

@Composable
private fun BookRow(book: ImportedBook, icon: ImageVector, tint: Color) {
    val ink = InkTheme.colors
    val authors = book.authors.filter { it.isNotBlank() }
        .takeIf { it.isNotEmpty() }
        ?.joinToString(", ")
        ?: stringResource(R.string.import_unknown_author)
    Row(
        Modifier.fillMaxWidth().padding(vertical = InkSpace.s2),
        horizontalArrangement = Arrangement.spacedBy(InkSpace.s3),
    ) {
        Icon(icon, contentDescription = null, tint = tint, modifier = Modifier.padding(top = 2.dp).size(18.dp))
        Column(Modifier.weight(1f)) {
            // Titles wrap rather than truncate: a CJK title carries its
            // meaning in every character, and clipping one is a mistranslation.
            Text(book.title, style = InkType.ui, color = ink.textBody)
            Text(authors, style = InkType.caption, color = ink.textTertiary)
        }
    }
}

@Composable
private fun FailureRow(failure: ImportFailure) {
    val ink = InkTheme.colors
    val reason = when (failure.kind) {
        ImportFailureKind.UnsupportedFormat ->
            stringResource(R.string.import_fail_unsupported, failure.format.orEmpty())
        ImportFailureKind.UnknownFormat -> stringResource(R.string.import_fail_unknown_format)
        ImportFailureKind.BrokenBook -> stringResource(R.string.import_fail_broken)
        ImportFailureKind.DamagedArchive -> stringResource(R.string.import_fail_archive)
        ImportFailureKind.TooLarge -> stringResource(R.string.import_fail_too_large)
        ImportFailureKind.Unreadable -> stringResource(R.string.import_fail_unreadable)
        ImportFailureKind.LibraryError -> stringResource(R.string.import_fail_library)
    }
    Row(
        Modifier.fillMaxWidth().padding(vertical = InkSpace.s2),
        horizontalArrangement = Arrangement.spacedBy(InkSpace.s3),
    ) {
        Icon(
            Icons.Outlined.ErrorOutline,
            contentDescription = null,
            tint = ink.danger,
            modifier = Modifier.padding(top = 2.dp).size(18.dp),
        )
        Column(Modifier.weight(1f)) {
            Text(failure.name, style = InkType.ui, color = ink.textBody)
            Text(reason, style = InkType.caption, color = ink.textTertiary)
        }
    }
}
