package app.inkuna.android.ui.components

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.material3.minimumInteractiveComponentSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.inkuna.android.ui.theme.InkMotion
import app.inkuna.android.ui.theme.InkRadius
import app.inkuna.android.ui.theme.InkTheme
import app.inkuna.android.ui.theme.InkType

/** Filter chip: accent-filled when selected, hairline outline otherwise. */
@Composable
fun InkChip(
    text: String,
    selected: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    icon: ImageVector? = null,
) {
    val ink = InkTheme.colors
    val bg by animateColorAsState(
        if (selected) ink.accentFill else ink.bgSurface,
        tween(InkMotion.durFast, easing = InkMotion.easeQuiet),
        label = "chipBg",
    )
    val fg by animateColorAsState(
        if (selected) ink.accentInk else ink.textSecondary,
        tween(InkMotion.durFast, easing = InkMotion.easeQuiet),
        label = "chipFg",
    )
    Row(
        modifier = modifier
            // The pill stays the design's height; the hit area grows to the
            // 48dp Android floor around it.
            .minimumInteractiveComponentSize()
            .clip(InkRadius.pillShape)
            .background(bg)
            .border(1.dp, if (selected) Color.Transparent else ink.borderHairline, InkRadius.pillShape)
            // A filter reads as a choice, not a command: TalkBack must hear
            // which chip is active.
            .selectable(selected = selected, role = Role.Tab, onClick = onClick)
            .padding(horizontal = 14.dp, vertical = 7.dp),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (icon != null) {
            Icon(icon, contentDescription = null, tint = fg, modifier = Modifier.size(16.dp))
        }
        Text(text, style = InkType.label, color = fg)
    }
}

/** Bare circular icon button; accent-tinted soft fill when active. */
@Composable
fun InkIconButton(
    icon: ImageVector,
    contentDescription: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    active: Boolean = false,
    size: Dp = 40.dp,
    iconSize: Dp = 22.dp,
    tint: Color = Color.Unspecified,
) {
    val ink = InkTheme.colors
    Box(
        modifier = modifier
            .minimumInteractiveComponentSize()
            .size(size)
            .clip(InkRadius.pillShape)
            .background(if (active) ink.accentSoft else Color.Transparent)
            .clickable(role = Role.Button, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            icon,
            contentDescription = contentDescription,
            tint = when {
                tint != Color.Unspecified -> tint
                active -> ink.accentText
                else -> ink.textBody
            },
            modifier = Modifier.size(iconSize),
        )
    }
}

/** Thin reading-progress bar: 3dp track, accent fill, settling animation. */
@Composable
fun InkProgressBar(
    progress: Int,
    modifier: Modifier = Modifier,
) {
    val ink = InkTheme.colors
    val fraction by animateFloatAsState(
        targetValue = (progress.coerceIn(0, 100)) / 100f,
        animationSpec = tween(InkMotion.durSlow, easing = InkMotion.easePage),
        label = "progress",
    )
    Box(
        modifier = modifier
            .height(3.dp)
            .clip(InkRadius.pillShape)
            .background(ink.bgRecessed),
    ) {
        Box(
            Modifier
                .fillMaxHeight()
                .fillMaxWidth(fraction)
                .clip(InkRadius.pillShape)
                .background(ink.accent),
        )
    }
}

/**
 * Recessed pill segmented control with a raised selected segment.
 * Addressed by index, never by label: two segments can share a translation.
 */
@Composable
fun InkSegmentedControl(
    options: List<String>,
    selectedIndex: Int,
    onSelect: (Int) -> Unit,
    modifier: Modifier = Modifier,
) {
    val ink = InkTheme.colors
    Row(
        modifier = modifier
            .clip(InkRadius.pillShape)
            .background(ink.bgRecessed)
            .padding(3.dp)
            .selectableGroup(),
        horizontalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        options.forEachIndexed { index, option ->
            val sel = index == selectedIndex
            val bg by animateColorAsState(
                if (sel) ink.bgRaised else Color.Transparent,
                tween(InkMotion.durFast, easing = InkMotion.easeQuiet),
                label = "segBg",
            )
            Box(
                modifier = Modifier
                    .weight(1f)
                    .heightIn(min = 48.dp)
                    .then(if (sel) Modifier.inkShadow(1.dp, InkRadius.pillShape) else Modifier)
                    .clip(InkRadius.pillShape)
                    .background(bg)
                    .selectable(selected = sel, role = Role.Tab) { onSelect(index) }
                    .padding(vertical = 7.dp),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    option,
                    style = InkType.label,
                    color = if (sel) ink.textBody else ink.textSecondary,
                )
            }
        }
    }
}
