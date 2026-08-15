package app.inkuna.android.ui.components

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.material3.minimumInteractiveComponentSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import app.inkuna.android.ui.theme.InkMotion
import app.inkuna.android.ui.theme.InkRadius
import app.inkuna.android.ui.theme.InkTheme
import app.inkuna.android.ui.theme.InkType

enum class InkButtonVariant { Primary, Secondary, Ghost }
enum class InkButtonSize { Small, Medium, Large }

/** Pill button from the design system: quiet press scale, no bounce. */
@Composable
fun InkButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    variant: InkButtonVariant = InkButtonVariant.Primary,
    size: InkButtonSize = InkButtonSize.Medium,
    icon: ImageVector? = null,
    enabled: Boolean = true,
) {
    val ink = InkTheme.colors
    val (container, content) = when (variant) {
        InkButtonVariant.Primary -> ink.accentFill to ink.accentInk
        InkButtonVariant.Secondary -> ink.accentSoft to ink.accentText
        InkButtonVariant.Ghost -> Color.Transparent to ink.textBody
    }
    val padding = when (size) {
        InkButtonSize.Small -> PaddingValues(horizontal = 14.dp, vertical = 7.dp)
        InkButtonSize.Medium -> PaddingValues(horizontal = 20.dp, vertical = 10.dp)
        InkButtonSize.Large -> PaddingValues(horizontal = 26.dp, vertical = 14.dp)
    }
    val textStyle = if (size == InkButtonSize.Small) InkType.label else InkType.ui
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val scale by animateFloatAsState(
        targetValue = if (pressed && enabled) 0.97f else 1f,
        animationSpec = tween(InkMotion.durFast, easing = InkMotion.easeQuiet),
        label = "buttonPress",
    )
    Row(
        modifier = modifier
            // Small/Medium pills measure under 48dp; the hit area is padded
            // out to the Android floor without changing the drawn pill.
            .minimumInteractiveComponentSize()
            .graphicsLayer {
                scaleX = scale
                scaleY = scale
                alpha = if (enabled) 1f else 0.4f
            }
            .clip(InkRadius.pillShape)
            .background(container)
            .clickable(
                interactionSource = interaction,
                indication = null,
                enabled = enabled,
                role = Role.Button,
                onClick = onClick,
            )
            .padding(padding),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (icon != null) {
            Icon(icon, contentDescription = null, tint = content, modifier = Modifier.size(18.dp))
        }
        Text(text, style = textStyle, color = content)
    }
}
