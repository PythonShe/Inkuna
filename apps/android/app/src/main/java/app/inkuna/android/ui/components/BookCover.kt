package app.inkuna.android.ui.components

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.times
import app.inkuna.android.ui.theme.InkRadius
import app.inkuna.android.ui.theme.InkSans
import app.inkuna.android.ui.theme.InkSerif

/** Placeholder cover palettes from the design system's BookCover component. */
private val coverPalettes = listOf(
    Color(0xFF2E3440) to Color(0xFFD8DEE9),
    Color(0xFF4A3B2A) to Color(0xFFEFE7D9),
    Color(0xFF5C3A38) to Color(0xFFF2E4DC),
    Color(0xFF33463C) to Color(0xFFE4EDE6),
    Color(0xFF3A3A55) to Color(0xFFE4E4F0),
    Color(0xFF6B5537) to Color(0xFFF6EDD9),
)

/**
 * A book cover: the core-extracted art at [coverPath] when the book ships
 * with any, faded in over the typographic title/author placeholder on a
 * seeded palette that stands while the art decodes and for books without.
 * Covers stay rectangular (radius-xs) — books are not app icons.
 */
@Composable
fun BookCover(
    title: String,
    author: String,
    width: Dp,
    seed: Int,
    coverPath: String? = null,
    modifier: Modifier = Modifier,
) {
    val height = width * 1.5f
    val (bg, fg) = coverPalettes[seed % coverPalettes.size]
    val pad = maxOf(8.dp, width * 0.1f)
    // Cover lettering is artwork inside a fixed-dp tile, not content: it is
    // sized off density so it cannot outgrow the tile at large font scales,
    // and it is bounded so long titles ellipsize instead of spilling.
    val density = LocalDensity.current
    val titleSize = with(density) { maxOf(11.dp, width * 0.13f).toSp() }
    val authorSize = with(density) { maxOf(8.dp, width * 0.085f).toSp() }
    val art by rememberCoverArt(coverPath, width)
    Box(
        modifier = modifier
            .width(width)
            .height(height)
            .inkShadow(4.dp, InkRadius.xsShape)
            .clip(InkRadius.xsShape)
            .background(bg)
            // The title and author are always repeated by the row or screen
            // around the cover; announcing them twice is noise.
            .clearAndSetSemantics {},
    ) {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(pad),
            verticalArrangement = androidx.compose.foundation.layout.Arrangement.SpaceBetween,
        ) {
            // Below ~48dp the typographic cover degrades into overflow
            // noise; a thumbnail is just the palette.
            if (width >= 48.dp) {
                Text(
                    text = title,
                    fontFamily = InkSerif,
                    fontWeight = FontWeight.Medium,
                    fontSize = titleSize,
                    lineHeight = titleSize * 1.25f,
                    color = fg,
                    maxLines = 3,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    text = author,
                    fontFamily = InkSans,
                    fontWeight = FontWeight.Normal,
                    fontSize = authorSize,
                    color = fg.copy(alpha = 0.8f),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        AnimatedVisibility(visible = art != null, enter = fadeIn(), exit = fadeOut()) {
            art?.let { bitmap ->
                Image(
                    bitmap = bitmap,
                    contentDescription = null,
                    contentScale = ContentScale.Crop,
                    modifier = Modifier.fillMaxSize(),
                )
            }
        }
    }
}
