package app.inkuna.android.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
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
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.inkuna.android.ui.stats.hairlineThickness
import app.inkuna.android.ui.theme.InkRadius
import app.inkuna.android.ui.theme.InkSpace
import app.inkuna.android.ui.theme.InkTheme
import app.inkuna.android.ui.theme.InkType

/**
 * Elevation shadow in ink rather than pure black, mirroring the iOS
 * `InkShadow` tokens: a warm cast on paper, full black at night where a
 * tinted shadow would disappear into the ground.
 */
@Composable
fun Modifier.inkShadow(elevation: Dp, shape: Shape): Modifier {
    val cast = if (InkTheme.colors.isNight) Color.Black else Color(0xFF1E1911)
    return shadow(elevation = elevation, shape = shape, ambientColor = cast, spotColor = cast)
}

/**
 * Floating chrome surface: near-opaque raised background standing in for
 * the design's backdrop blur (Compose has no cheap cross-surface blur, so
 * legibility wins over translucency).
 */
@Composable
fun glassModifier(shape: Shape = InkRadius.pillShape): Modifier {
    val ink = InkTheme.colors
    return Modifier
        .inkShadow(6.dp, shape)
        .clip(shape)
        .background(ink.bgRaised.copy(alpha = 0.97f))
}

/** Surface + shadow + clip: the grouped-list card idiom of the settings
 *  sheets (Account, reader Customize). */
@Composable
fun GroupCard(content: @Composable () -> Unit) {
    val ink = InkTheme.colors
    Column(
        Modifier
            .fillMaxWidth()
            .inkShadow(4.dp, InkRadius.lgShape)
            .clip(InkRadius.lgShape)
            .background(ink.bgSurface),
    ) {
        content()
    }
}

/** The grouped card's inset hairline, drawn between rows. */
@Composable
fun GroupHairline() {
    val ink = InkTheme.colors
    Box(
        Modifier
            .fillMaxWidth()
            .padding(start = InkSpace.s4)
            .height(hairlineThickness())
            .background(ink.borderHairline),
    )
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
            .inkShadow(10.dp, InkRadius.pillShape)
            .clip(InkRadius.pillShape)
            .background(Color(0xFF241F17).copy(alpha = 0.92f))
            // Announced, not just shown: the confirmation is the only signal
            // that the bookmark landed.
            .semantics { liveRegion = LiveRegionMode.Polite },
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
