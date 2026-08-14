package app.inkuna.android.ui.reader

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import app.inkuna.android.ui.components.glassModifier
import app.inkuna.android.ui.theme.InkMotion
import app.inkuna.android.ui.theme.InkRadius
import app.inkuna.android.ui.theme.InkTheme
import app.inkuna.android.ui.theme.InkType

/** 46dp circular glass control used by the reader's floating chrome. */
@Composable
fun ReaderGlassButton(
    icon: ImageVector,
    contentDescription: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val ink = InkTheme.colors
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val scale by animateFloatAsState(
        if (pressed) 0.97f else 1f,
        tween(InkMotion.durFast, easing = InkMotion.easeQuiet),
        label = "glassPress",
    )
    Box(
        modifier = modifier
            .graphicsLayer {
                scaleX = scale
                scaleY = scale
            }
            .size(46.dp)
            .then(glassModifier(InkRadius.pillShape))
            .clickable(
                interactionSource = interaction,
                indication = null,
                role = Role.Button,
                onClick = onClick,
            ),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            icon,
            contentDescription = contentDescription,
            tint = ink.textDisplay,
            modifier = Modifier.size(20.dp),
        )
    }
}

/** Wide glass pill in the reading menu: label left, quiet icon right. */
@Composable
fun ReaderMenuPill(
    text: String,
    icon: ImageVector,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val ink = InkTheme.colors
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val scale by animateFloatAsState(
        if (pressed) 0.98f else 1f,
        tween(InkMotion.durFast, easing = InkMotion.easeQuiet),
        label = "pillPress",
    )
    Row(
        modifier = modifier
            .graphicsLayer {
                scaleX = scale
                scaleY = scale
                alpha = if (pressed) 0.85f else 1f
            }
            .widthIn(min = 198.dp)
            .then(glassModifier(InkRadius.pillShape))
            .clickable(
                interactionSource = interaction,
                indication = null,
                role = Role.Button,
                onClick = onClick,
            )
            .padding(start = 18.dp, end = 15.dp, top = 13.dp, bottom = 13.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(text, style = InkType.ui, color = ink.textDisplay)
        Icon(
            icon,
            contentDescription = null,
            tint = ink.textSecondary,
            modifier = Modifier
                .padding(start = 14.dp)
                .size(20.dp),
        )
    }
}
