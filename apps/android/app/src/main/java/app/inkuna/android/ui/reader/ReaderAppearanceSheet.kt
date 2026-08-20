package app.inkuna.android.ui.reader

import androidx.compose.foundation.background
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
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowBack
import androidx.compose.material.icons.outlined.Check
import androidx.compose.material.icons.outlined.ExpandLess
import androidx.compose.material.icons.outlined.ExpandMore
import androidx.compose.material.icons.outlined.FormatBold
import androidx.compose.material.icons.outlined.RestartAlt
import androidx.compose.material.icons.outlined.TextFields
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.minimumInteractiveComponentSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp
import app.inkuna.android.R
import app.inkuna.android.model.AppSettings
import app.inkuna.android.ui.components.GroupCard
import app.inkuna.android.ui.components.GroupHairline
import app.inkuna.android.ui.components.InkButton
import app.inkuna.android.ui.components.InkButtonVariant
import app.inkuna.android.ui.components.inkShadow
import app.inkuna.android.ui.main.EyebrowText
import app.inkuna.android.ui.theme.InkRadius
import app.inkuna.android.ui.theme.InkSpace
import app.inkuna.android.ui.theme.InkTheme
import app.inkuna.android.ui.theme.InkType
import app.inkuna.android.ui.theme.ReadingFont
import app.inkuna.android.ui.theme.ReadingTheme
import app.inkuna.android.ui.theme.composeFamily
import java.util.Locale
import kotlin.math.roundToInt

/**
 * The Customize panel — level 2 of the Theme & type sheet. A live preview
 * on the reading surface, the font roster with own-typeface specimens,
 * the bold toggle, and the four layout sliders; everything applies
 * through the reader's own stylesheet, previewing per step and
 * committing on release.
 */
@Composable
internal fun CustomizePanel(
    snapshot: AppSettings.Snapshot,
    settings: AppSettings,
    appearance: ReaderAppearanceController,
    onBack: () -> Unit,
    onClose: () -> Unit,
) {
    val ink = InkTheme.colors
    val haptics = LocalHapticFeedback.current

    // The uncommitted draft the preview card and sliders render; committed
    // values land in the store and flow back through the snapshot.
    var draft by remember { mutableStateOf(ReaderTypeDraft.from(snapshot)) }

    val fallback = stringResource(R.string.reader_preview_fallback)
    val resetLabel = stringResource(R.string.a11y_reset_defaults)
    var phrase by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(Unit) { phrase = appearance.currentPhrase() ?: fallback }

    fun commit(next: ReaderTypeDraft, write: () -> Unit) {
        draft = next
        write()
    }

    // Slider releases must close the anchor session themselves: a commit of
    // the value already stored never re-emits the snapshot, so the
    // applyCommitted path would leave the anchor stranded.
    fun commitSlider(next: ReaderTypeDraft, write: () -> Unit) {
        commit(next, write)
        appearance.endPreview()
    }

    Column(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = InkSpace.pageMargin)
            .padding(bottom = InkSpace.s6),
    ) {
        PanelTopBar(
            title = stringResource(R.string.reader_customize),
            onBack = onBack,
            onClose = onClose,
        )
        Column(
            Modifier
                .weight(1f, fill = false)
                .verticalScroll(rememberScrollState()),
        ) {
            Spacer(Modifier.height(InkSpace.s4))
            AppearancePreviewCard(
                phrase = phrase ?: fallback,
                theme = snapshot.readingTheme,
                textSizeSp = AppSettings.TEXT_SIZE_STEPS[snapshot.textSizeStep],
                draft = draft,
            )
            Spacer(Modifier.height(InkSpace.s5))

            EyebrowText(stringResource(R.string.reader_type_section))
            Spacer(Modifier.height(InkSpace.s3))
            GroupCard {
                FontRow(
                    current = draft.font,
                    onPick = { font ->
                        haptics.performHapticFeedback(HapticFeedbackType.SegmentTick)
                        commit(draft.copy(font = font)) { settings.setReadingFont(font) }
                    },
                )
                GroupHairline()
                AppearanceToggleRow(
                    icon = Icons.Outlined.FormatBold,
                    title = stringResource(R.string.reader_bold_text),
                    a11yLabel = stringResource(R.string.a11y_bold_text),
                    checked = draft.bold,
                    onCheckedChange = { on ->
                        haptics.performHapticFeedback(HapticFeedbackType.SegmentTick)
                        commit(draft.copy(bold = on)) { settings.setReadingBold(on) }
                    },
                )
            }
            Spacer(Modifier.height(InkSpace.s5))

            EyebrowText(stringResource(R.string.reader_layout_section))
            Spacer(Modifier.height(InkSpace.s3))
            SteppedSliderRow(
                label = stringResource(R.string.reader_line_spacing),
                valueText = String.format(Locale.getDefault(), "%.2f", draft.lineSpacing),
                a11yLabel = stringResource(R.string.a11y_line_spacing),
                value = draft.lineSpacing,
                range = AppSettings.MIN_LINE_SPACING..AppSettings.MAX_LINE_SPACING,
                step = 0.05f,
                onBegin = { appearance.beginPreview() },
                onPreview = { value ->
                    draft = draft.copy(lineSpacing = value)
                    appearance.preview(draft)
                },
                onCommit = { value ->
                    commitSlider(draft.copy(lineSpacing = value)) { settings.setLineSpacing(value) }
                },
            )
            Spacer(Modifier.height(InkSpace.s4))
            SteppedSliderRow(
                label = stringResource(R.string.reader_letter_spacing),
                valueText = percentText(draft.letterSpacing),
                a11yLabel = stringResource(R.string.a11y_letter_spacing),
                value = draft.letterSpacing,
                range = 0f..AppSettings.MAX_LETTER_SPACING,
                step = 0.005f,
                onBegin = { appearance.beginPreview() },
                onPreview = { value ->
                    draft = draft.copy(letterSpacing = value)
                    appearance.preview(draft)
                },
                onCommit = { value ->
                    commitSlider(draft.copy(letterSpacing = value)) { settings.setLetterSpacing(value) }
                },
            )
            Spacer(Modifier.height(InkSpace.s4))
            SteppedSliderRow(
                label = stringResource(R.string.reader_word_spacing),
                valueText = percentText(draft.wordSpacing),
                a11yLabel = stringResource(R.string.a11y_word_spacing),
                value = draft.wordSpacing,
                range = 0f..AppSettings.MAX_WORD_SPACING,
                step = 0.025f,
                onBegin = { appearance.beginPreview() },
                onPreview = { value ->
                    draft = draft.copy(wordSpacing = value)
                    appearance.preview(draft)
                },
                onCommit = { value ->
                    commitSlider(draft.copy(wordSpacing = value)) { settings.setWordSpacing(value) }
                },
            )
            Spacer(Modifier.height(InkSpace.s4))
            SteppedSliderRow(
                label = stringResource(R.string.reader_margins),
                valueText = stringResource(R.string.reader_value_px, draft.margins.toString()),
                a11yLabel = stringResource(R.string.a11y_margins),
                value = draft.margins.toFloat(),
                range = AppSettings.MIN_READING_MARGINS.toFloat()..AppSettings.MAX_READING_MARGINS.toFloat(),
                step = 2f,
                onBegin = { appearance.beginPreview() },
                onPreview = { value ->
                    draft = draft.copy(margins = value.roundToInt())
                    appearance.preview(draft)
                },
                onCommit = { value ->
                    commitSlider(draft.copy(margins = value.roundToInt())) {
                        settings.setReadingMargins(value.roundToInt())
                    }
                },
            )
            Spacer(Modifier.height(InkSpace.s6))

            InkButton(
                text = stringResource(R.string.reader_reset_defaults),
                onClick = {
                    haptics.performHapticFeedback(HapticFeedbackType.SegmentTick)
                    draft = ReaderTypeDraft(
                        font = ReadingFont.DEFAULT,
                        bold = false,
                        lineSpacing = AppSettings.DEFAULT_LINE_SPACING,
                        letterSpacing = 0f,
                        wordSpacing = 0f,
                        margins = AppSettings.DEFAULT_READING_MARGINS,
                    )
                    settings.setReadingAppearanceDefaults()
                },
                variant = InkButtonVariant.Recessed,
                icon = Icons.Outlined.RestartAlt,
                modifier = Modifier
                    .fillMaxWidth()
                    .semantics { contentDescription = resetLabel },
            )
        }
    }
}

/** "0%" … "6%" style readout for the em-valued spacing sliders. */
private fun percentText(em: Float): String {
    val percent = em * 100f
    val text = if (percent == percent.roundToInt().toFloat()) {
        percent.roundToInt().toString()
    } else {
        String.format(Locale.getDefault(), "%.1f", percent)
    }
    return "$text%"
}

@Composable
private fun PanelTopBar(title: String, onBack: () -> Unit, onClose: () -> Unit) {
    val ink = InkTheme.colors
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(InkSpace.s2),
        modifier = Modifier.fillMaxWidth(),
    ) {
        Box(
            modifier = Modifier
                .minimumInteractiveComponentSize()
                .size(32.dp)
                .clip(InkRadius.pillShape)
                .background(ink.bgRecessed)
                .clickable(role = Role.Button, onClick = onBack),
            contentAlignment = Alignment.Center,
        ) {
            Icon(
                Icons.AutoMirrored.Outlined.ArrowBack,
                contentDescription = stringResource(R.string.a11y_back_to_theme_type),
                tint = ink.textSecondary,
                modifier = Modifier.size(18.dp),
            )
        }
        Text(
            title,
            style = InkType.heading,
            color = ink.textDisplay,
            modifier = Modifier.weight(1f),
        )
        SheetCloseButton(onClick = onClose)
    }
}

/**
 * The design's preview slice: the current reading surface with every live
 * typography value applied. Word spacing is the one control Compose text
 * cannot render — it still applies live to the page behind the sheet.
 * Margins are deliberately not applied; the card is about type.
 */
@Composable
private fun AppearancePreviewCard(
    phrase: String,
    theme: ReadingTheme,
    textSizeSp: Float,
    draft: ReaderTypeDraft,
) {
    val a11yPreview = stringResource(R.string.a11y_preview_sample)
    Box(
        Modifier
            .fillMaxWidth()
            .inkShadow(2.dp, InkRadius.mdShape)
            .clip(InkRadius.mdShape)
            .background(theme.background)
            // A minimum, not a maximum: the card keeps its designed slice
            // height but grows when large type or wide line spacing needs
            // more room, so the third line is never cut mid-glyph.
            .heightIn(min = 118.dp)
            .clearAndSetSemantics { contentDescription = "$a11yPreview: $phrase" }
            .padding(horizontal = 18.dp, vertical = 16.dp),
    ) {
        Text(
            phrase,
            fontFamily = draft.font.composeFamily(),
            fontWeight = if (draft.bold) FontWeight.SemiBold else FontWeight.Normal,
            fontSize = textSizeSp.sp,
            lineHeight = (textSizeSp * draft.lineSpacing).sp,
            letterSpacing = draft.letterSpacing.em,
            color = theme.foreground,
            maxLines = 3,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/**
 * The Font selector: a value row that opens a native Material 3 menu
 * anchored to the whole row and matched to its measured width, so the
 * menu drops straight down the selector instead of hugging one side.
 * The menu is the platform's own picker — items are
 * natively focusable and readable by TalkBack — so the row keeps only
 * its own "Font: <value>" announcement, and each item still renders in
 * its own typeface so the roster reads as a specimen sheet.
 */
@Composable
private fun FontRow(current: ReadingFont, onPick: (ReadingFont) -> Unit) {
    val ink = InkTheme.colors
    val title = stringResource(R.string.reader_font)
    val value = stringResource(current.nameRes)
    var expanded by remember { mutableStateOf(false) }
    // The anchor Box spans the row, and the menu takes the row's measured
    // width, so it opens as a full-width sheet under the selector.
    val density = LocalDensity.current
    var rowWidth by remember { mutableStateOf(0.dp) }
    Box(
        Modifier
            .fillMaxWidth()
            .onSizeChanged { size ->
                val width = with(density) { size.width.toDp() }
                if (width != rowWidth) rowWidth = width
            },
    ) {
        Row(
            Modifier
                .fillMaxWidth()
                .clickable { expanded = true }
                .clearAndSetSemantics {
                    contentDescription = "$title: $value"
                    role = Role.Button
                }
                .padding(horizontal = InkSpace.s4, vertical = 14.dp),
            horizontalArrangement = Arrangement.spacedBy(InkSpace.s4),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(
                Icons.Outlined.TextFields,
                contentDescription = null,
                tint = ink.textSecondary,
                modifier = Modifier.size(20.dp),
            )
            Text(title, style = InkType.ui, color = ink.textDisplay, modifier = Modifier.weight(1f))
            Text(value, style = InkType.caption, color = ink.textTertiary)
            Icon(
                if (expanded) Icons.Outlined.ExpandLess else Icons.Outlined.ExpandMore,
                contentDescription = null,
                tint = ink.textTertiary,
                modifier = Modifier.size(18.dp),
            )
        }
        DropdownMenu(
            expanded = expanded,
            onDismissRequest = { expanded = false },
            containerColor = ink.bgRaised,
            shape = InkRadius.mdShape,
            modifier = if (rowWidth > 0.dp) Modifier.width(rowWidth) else Modifier,
        ) {
            ReadingFont.entries.forEach { font ->
                val chosen = font == current
                val name = stringResource(font.nameRes)
                DropdownMenuItem(
                    text = {
                        Text(
                            name,
                            fontFamily = font.composeFamily(),
                            fontWeight = if (chosen) FontWeight.Medium else FontWeight.Normal,
                            style = InkType.ui,
                            color = ink.textDisplay,
                        )
                    },
                    onClick = {
                        expanded = false
                        onPick(font)
                    },
                    trailingIcon = {
                        // The checkmark's space is always reserved: selection
                        // never shifts the item's text.
                        Box(Modifier.size(20.dp), contentAlignment = Alignment.Center) {
                            if (chosen) {
                                Icon(
                                    Icons.Outlined.Check,
                                    contentDescription = null,
                                    tint = ink.accent,
                                    modifier = Modifier.size(18.dp),
                                )
                            }
                        }
                    },
                    modifier = Modifier.semantics { selected = chosen },
                )
            }
        }
    }
}

@Composable
private fun AppearanceToggleRow(
    icon: ImageVector,
    title: String,
    a11yLabel: String,
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
) {
    val ink = InkTheme.colors
    Row(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = InkSpace.s4, vertical = 6.dp),
        horizontalArrangement = Arrangement.spacedBy(InkSpace.s4),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(icon, contentDescription = null, tint = ink.textSecondary, modifier = Modifier.size(20.dp))
        Text(title, style = InkType.ui, color = ink.textDisplay, modifier = Modifier.weight(1f))
        Switch(
            checked = checked,
            onCheckedChange = onCheckedChange,
            colors = SwitchDefaults.colors(checkedTrackColor = ink.accentFill),
            modifier = Modifier.semantics { contentDescription = a11yLabel },
        )
    }
}

/**
 * A labeled, discretely-stepped slider: title left, live readout right,
 * native M3 slider below. Previews per step with a haptic tick, commits
 * once on release — the BrightnessRow discipline.
 */
@Composable
private fun SteppedSliderRow(
    label: String,
    valueText: String,
    a11yLabel: String,
    value: Float,
    range: ClosedFloatingPointRange<Float>,
    step: Float,
    onBegin: () -> Unit,
    onPreview: (Float) -> Unit,
    onCommit: (Float) -> Unit,
) {
    val ink = InkTheme.colors
    val haptics = LocalHapticFeedback.current
    val steps = (((range.endInclusive - range.start) / step).roundToInt() - 1).coerceAtLeast(0)
    // Both re-key on the external value: Reset rewrites the draft without a
    // gesture, and a stale lastTick would make a bare tap commit it.
    var live by remember(value) { mutableFloatStateOf(value) }
    var lastTick by remember(value) { mutableIntStateOf(((value - range.start) / step).roundToInt()) }
    var dragging by remember { mutableStateOf(false) }
    Column(Modifier.fillMaxWidth()) {
        Row(Modifier.fillMaxWidth()) {
            Text(label, style = InkType.ui, color = ink.textDisplay, modifier = Modifier.weight(1f))
            Text(valueText, style = InkType.caption, color = ink.textTertiary)
        }
        Slider(
            value = live,
            onValueChange = { raw ->
                if (!dragging) {
                    dragging = true
                    onBegin()
                }
                live = raw
                val tick = ((raw - range.start) / step).roundToInt()
                if (tick != lastTick) {
                    lastTick = tick
                    haptics.performHapticFeedback(HapticFeedbackType.SegmentTick)
                    onPreview(range.start + tick * step)
                }
            },
            onValueChangeFinished = {
                // A tap that lands on the current tick fires no
                // onValueChange first; committing there would write a value
                // the user never chose, with no anchor session open.
                if (dragging) {
                    dragging = false
                    onCommit(range.start + lastTick * step)
                }
            },
            valueRange = range,
            steps = steps,
            colors = SliderDefaults.colors(
                thumbColor = ink.accent,
                activeTrackColor = ink.accent,
                inactiveTrackColor = ink.bgRecessed,
            ),
            modifier = Modifier
                .fillMaxWidth()
                .semantics {
                    contentDescription = a11yLabel
                    stateDescription = valueText
                },
        )
    }
}
