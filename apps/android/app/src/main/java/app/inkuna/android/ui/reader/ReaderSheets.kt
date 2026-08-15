package app.inkuna.android.ui.reader

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.BrightnessHigh
import androidx.compose.material.icons.outlined.BrightnessLow
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.SheetState
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.minimumInteractiveComponentSize
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.inkuna.android.R
import app.inkuna.android.model.AppSettings
import app.inkuna.android.ui.components.BookCover
import app.inkuna.core.Chapter as CoreChapter
import app.inkuna.core.Publication as CorePublication
import kotlin.math.abs
import app.inkuna.android.ui.components.inkShadow
import app.inkuna.android.ui.stats.hairlineThickness
import app.inkuna.android.ui.theme.InkRadius
import app.inkuna.android.ui.theme.InkSerif
import app.inkuna.android.ui.theme.InkSpace
import app.inkuna.android.ui.theme.InkTheme
import app.inkuna.android.ui.theme.InkType
import app.inkuna.android.ui.theme.ReadingTheme
import kotlinx.coroutines.launch
import androidx.compose.foundation.shape.RoundedCornerShape

private val sheetShape = RoundedCornerShape(topStart = 16.dp, topEnd = 16.dp)

/**
 * Small recessed circular close control shared by the reader overlays. The
 * disc stays 32dp; the hit area grows to the 48dp Android floor around it.
 */
@Composable
fun SheetCloseButton(onClick: () -> Unit, modifier: Modifier = Modifier) {
    val ink = InkTheme.colors
    Box(
        modifier = modifier
            .minimumInteractiveComponentSize()
            .size(32.dp)
            .clip(InkRadius.pillShape)
            .background(ink.bgRecessed)
            .clickable(role = Role.Button, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            Icons.Outlined.Close,
            contentDescription = stringResource(R.string.a11y_close),
            tint = ink.textSecondary,
            modifier = Modifier.size(18.dp),
        )
    }
}

/**
 * Explicit dismissals must run the sheet's own hide animation first —
 * dropping the composable straight out snaps the sheet and scrim away in a
 * single frame while a drag-dismiss of the same sheet glides.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun rememberSheetDismiss(
    sheetState: SheetState,
    onDismiss: () -> Unit,
): () -> Unit {
    val scope = rememberCoroutineScope()
    return {
        scope.launch { sheetState.hide() }.invokeOnCompletion {
            if (!sheetState.isVisible) onDismiss()
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ThemeTypeSheet(
    snapshot: AppSettings.Snapshot,
    settings: AppSettings,
    onBrightnessPreview: (Float) -> Unit,
    onDismiss: () -> Unit,
) {
    val ink = InkTheme.colors
    val scope = rememberCoroutineScope()
    val haptics = LocalHapticFeedback.current
    val sheetState = rememberModalBottomSheetState()
    val dismiss = rememberSheetDismiss(sheetState, onDismiss)

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        containerColor = ink.bgSurface,
        scrimColor = ink.scrim,
        shape = sheetShape,
    ) {
        Column(
            Modifier.padding(
                start = InkSpace.pageMargin,
                end = InkSpace.pageMargin,
                bottom = InkSpace.s6,
            )
        ) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.fillMaxWidth(),
            ) {
                Text(
                    stringResource(R.string.reader_menu_theme_type),
                    style = InkType.heading,
                    color = ink.textDisplay,
                    modifier = Modifier.weight(1f),
                )
                SheetCloseButton(onClick = dismiss)
            }
            Spacer(Modifier.height(InkSpace.s4))
            TextSizeStepper(
                step = snapshot.textSizeStep,
                onStep = { newStep ->
                    haptics.performHapticFeedback(HapticFeedbackType.SegmentTick)
                    scope.launch { settings.setTextSizeStep(newStep) }
                },
            )
            Spacer(Modifier.height(14.dp))
            BrightnessRow(
                brightness = snapshot.brightness,
                onPreview = onBrightnessPreview,
                onCommit = { value -> scope.launch { settings.setBrightness(value) } },
            )
            Spacer(Modifier.height(InkSpace.s4))
            ThemeGrid(
                selected = snapshot.readingTheme,
                onPick = { theme ->
                    haptics.performHapticFeedback(HapticFeedbackType.SegmentTick)
                    scope.launch { settings.setReadingTheme(theme) }
                },
            )
        }
    }
}

/** Split A− / A+ stepper over the five-step reading scale. */
@Composable
private fun TextSizeStepper(step: Int, onStep: (Int) -> Unit) {
    val ink = InkTheme.colors
    val canShrink = step > 0
    val canGrow = step < AppSettings.TEXT_SIZE_STEPS.lastIndex
    // heightIn, not height: the A/A specimens are sp-sized, so a hard 40dp
    // beheads them on the one sheet an accessibility reader opens first.
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = 48.dp)
            .clip(InkRadius.pillShape)
            .background(ink.bgRecessed),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        StepperHalf(
            fontSize = 13.5f,
            enabled = canShrink,
            label = stringResource(R.string.a11y_smaller_text),
            onClick = { if (canShrink) onStep(step - 1) },
            modifier = Modifier.weight(1f),
        )
        Box(
            Modifier
                .padding(vertical = 12.dp)
                .width(hairlineThickness())
                .height(24.dp)
                .background(ink.borderHairline)
        )
        StepperHalf(
            fontSize = 19f,
            enabled = canGrow,
            label = stringResource(R.string.a11y_larger_text),
            onClick = { if (canGrow) onStep(step + 1) },
            modifier = Modifier.weight(1f),
        )
    }
}

@Composable
private fun StepperHalf(
    fontSize: Float,
    enabled: Boolean,
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val ink = InkTheme.colors
    Box(
        modifier = modifier
            .heightIn(min = 48.dp)
            .clickable(role = Role.Button, onClick = onClick)
            .clearAndSetSemantics {
                contentDescription = label
                role = Role.Button
            },
        contentAlignment = Alignment.Center,
    ) {
        Text(
            "A",
            fontFamily = InkSerif,
            fontWeight = FontWeight.Medium,
            fontSize = fontSize.sp,
            color = if (enabled) ink.textDisplay else ink.textTertiary,
        )
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun BrightnessRow(
    brightness: Float,
    onPreview: (Float) -> Unit,
    onCommit: (Float) -> Unit,
) {
    val ink = InkTheme.colors
    // The veil still follows the thumb — through an in-memory preview — but
    // only the released value reaches DataStore, which was otherwise taking
    // a whole-file read-modify-write per pointer sample.
    var value by remember { mutableFloatStateOf(brightness) }
    val a11yLabel = stringResource(R.string.a11y_page_brightness)
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(InkSpace.s3),
    ) {
        Icon(
            Icons.Outlined.BrightnessLow,
            contentDescription = null,
            tint = ink.textTertiary,
            modifier = Modifier.size(18.dp),
        )
        // Quiet thin track — the expressive default M3 slider is far too
        // heavy for a reading surface.
        val interaction = remember { MutableInteractionSource() }
        val colors = SliderDefaults.colors(
            thumbColor = ink.accent,
            activeTrackColor = ink.accent,
            inactiveTrackColor = ink.bgRecessed,
        )
        Slider(
            value = value,
            onValueChange = {
                value = it
                onPreview(it)
            },
            onValueChangeFinished = { onCommit(value) },
            interactionSource = interaction,
            thumb = {
                Box(
                    Modifier
                        .size(18.dp)
                        .clip(InkRadius.pillShape)
                        .background(ink.accent)
                )
            },
            track = { state ->
                SliderDefaults.Track(
                    sliderState = state,
                    colors = colors,
                    thumbTrackGapSize = 0.dp,
                    trackInsideCornerSize = 2.dp,
                    drawStopIndicator = null,
                    modifier = Modifier.height(4.dp),
                )
            },
            modifier = Modifier
                .weight(1f)
                .semantics { contentDescription = a11yLabel },
        )
        Icon(
            Icons.Outlined.BrightnessHigh,
            contentDescription = null,
            tint = ink.textTertiary,
            modifier = Modifier.size(18.dp),
        )
    }
}

@Composable
private fun ThemeGrid(selected: ReadingTheme, onPick: (ReadingTheme) -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        ReadingTheme.entries.chunked(2).forEach { rowThemes ->
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                rowThemes.forEach { theme ->
                    ThemeTile(
                        theme = theme,
                        chosen = theme == selected,
                        onPick = onPick,
                        modifier = Modifier.weight(1f),
                    )
                }
            }
        }
    }
}

@Composable
private fun ThemeTile(
    theme: ReadingTheme,
    chosen: Boolean,
    onPick: (ReadingTheme) -> Unit,
    modifier: Modifier = Modifier,
) {
    val ink = InkTheme.colors
    val name = stringResource(theme.nameRes)
    val tileLabel = stringResource(R.string.a11y_theme_tile, name, stringResource(theme.subtitleRes))
    Row(
        modifier = modifier
            .inkShadow(2.dp, InkRadius.mdShape)
            .clip(InkRadius.mdShape)
            .background(theme.background)
            .border(2.dp, if (chosen) ink.accent else Color.Transparent, InkRadius.mdShape)
            .clickable(role = Role.Button) { onPick(theme) }
            .semantics {
                contentDescription = tileLabel
                selected = chosen
            }
            .padding(horizontal = InkSpace.s4, vertical = 14.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.Bottom,
    ) {
        Text(
            "Aa",
            fontFamily = InkSerif,
            fontWeight = FontWeight.Medium,
            fontSize = 21.5.sp,
            color = theme.foreground,
        )
        Text(
            name,
            style = InkType.caption,
            color = theme.foreground.copy(alpha = 0.75f),
        )
    }
}

/**
 * The core's flattened TOC. Rows are numbered by TOC order and indented by
 * depth; the trailing "p. N" is the chapter resource's first synthetic
 * position — honest numbers, never invented pages — and is simply absent
 * until positions are known.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ContentsSheet(
    publication: CorePublication,
    chapters: List<ReaderViewModel.ReaderChapter>,
    currentChapterIndex: Int?,
    pageInfo: String,
    onSelect: (CoreChapter) -> Unit,
    onDismiss: () -> Unit,
) {
    val ink = InkTheme.colors
    val sheetState = rememberModalBottomSheetState()
    val dismiss = rememberSheetDismiss(sheetState, onDismiss)
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        containerColor = ink.bgSurface,
        scrimColor = ink.scrim,
        shape = sheetShape,
    ) {
        Column {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(13.dp),
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(start = InkSpace.s4, end = InkSpace.s4, bottom = InkSpace.s3),
            ) {
                BookCover(
                    title = publication.title,
                    author = publication.authors.joinToString(", "),
                    width = 34.dp,
                    seed = abs(publication.id.hashCode()),
                )
                Column(Modifier.weight(1f)) {
                    Text(
                        publication.title,
                        style = InkType.heading.copy(fontSize = 15.sp, lineHeight = 19.sp),
                        color = ink.textDisplay,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    Text(
                        pageInfo,
                        style = InkType.caption,
                        color = ink.textTertiary,
                        modifier = Modifier.padding(top = 2.dp),
                    )
                }
                SheetCloseButton(onClick = dismiss)
            }
            Box(
                Modifier
                    .fillMaxWidth()
                    .height(hairlineThickness())
                    .background(ink.borderHairline)
            )
            LazyColumn(
                modifier = Modifier.padding(horizontal = InkSpace.s4),
                contentPadding = androidx.compose.foundation.layout.PaddingValues(
                    top = 6.dp,
                    bottom = 14.dp,
                ),
            ) {
                itemsIndexed(
                    chapters,
                    key = { _, entry -> entry.chapter.id },
                ) { index, entry ->
                    val chapter = entry.chapter
                    val current = index == currentChapterIndex
                    val numeral = (chapter.idx + 1u).toString()
                    val rowLabel = if (entry.position != null) {
                        stringResource(
                            R.string.a11y_chapter_row, numeral, chapter.title, entry.position,
                        )
                    } else {
                        stringResource(R.string.a11y_chapter_row_no_page, numeral, chapter.title)
                    }
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(InkSpace.s3),
                        modifier = Modifier
                            .fillMaxWidth()
                            .clip(InkRadius.smShape)
                            .background(if (current) ink.accentSoft else Color.Transparent)
                            .clickable {
                                onSelect(chapter)
                                dismiss()
                            }
                            .clearAndSetSemantics {
                                contentDescription = rowLabel
                                role = Role.Button
                                selected = current
                            }
                            .padding(horizontal = 10.dp, vertical = 13.dp),
                    ) {
                        Text(
                            numeral,
                            style = InkType.caption,
                            color = if (current) ink.accentText else ink.textTertiary,
                            modifier = Modifier
                                .padding(start = (chapter.depth.toInt() * 12).dp)
                                .widthIn(min = 22.dp),
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
                        if (entry.position != null) {
                            Text(
                                stringResource(R.string.reader_chapter_page, entry.position),
                                style = InkType.caption,
                                color = ink.textTertiary,
                            )
                        }
                    }
                }
            }
        }
    }
}
