package app.inkuna.android.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.dp
import app.inkuna.android.ui.theme.InkRadius
import app.inkuna.android.ui.theme.InkTheme
import app.inkuna.android.ui.theme.InkType

/**
 * Floating chrome surface: near-opaque raised background standing in for
 * the design's backdrop blur (Compose has no cheap cross-surface blur;
 * 95% opacity over paper reads the same at this size).
 */
@Composable
fun glassModifier(shape: Shape = InkRadius.pillShape): Modifier {
    val ink = InkTheme.colors
    return Modifier
        .shadow(6.dp, shape)
        .clip(shape)
        .background(ink.bgRaised.copy(alpha = 0.95f))
}

/** Transient confirmation pill ("Bookmark placed."). Ink-dark in both themes. */
@Composable
fun InkToast(
    text: String,
    icon: ImageVector,
    modifier: Modifier = Modifier,
) {
    val ink = InkTheme.colors
    Row(
        modifier = modifier
            .shadow(10.dp, InkRadius.pillShape)
            .clip(InkRadius.pillShape)
            .background(Color(0xFF241F17).copy(alpha = 0.92f)),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Row(
            Modifier.padding(horizontal = 18.dp, vertical = 10.dp),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(icon, contentDescription = null, tint = ink.accent, modifier = Modifier.size(17.dp))
            Text(text, style = InkType.label, color = Color(0xFFF2EBDD))
        }
    }
}
