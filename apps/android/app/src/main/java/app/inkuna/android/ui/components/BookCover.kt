package app.inkuna.android.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
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
 * A book cover placeholder: typographic title/author over a seeded palette.
 * Covers stay rectangular (radius-xs) — books are not app icons.
 * TODO(core): render real cover art from the Rust core's metadata store.
 */
@Composable
fun BookCover(
    title: String,
    author: String,
    width: Dp,
    seed: Int,
    modifier: Modifier = Modifier,
) {
    val height = width * 1.5f
    val (bg, fg) = coverPalettes[seed % coverPalettes.size]
    val pad = maxOf(8.dp, width * 0.1f)
    Column(
        modifier = modifier
            .width(width)
            .height(height)
            .shadow(4.dp, InkRadius.xsShape)
            .clip(InkRadius.xsShape)
            .background(bg)
            .padding(pad),
        verticalArrangement = androidx.compose.foundation.layout.Arrangement.SpaceBetween,
    ) {
        // Below ~48dp the typographic cover degrades into overflow noise;
        // a thumbnail is just the palette.
        if (width < 48.dp) return@Column
        Text(
            text = title,
            fontFamily = InkSerif,
            fontWeight = FontWeight.Medium,
            fontSize = maxOf(11f, width.value * 0.13f).sp,
            lineHeight = maxOf(11f, width.value * 0.13f).sp * 1.25f,
            color = fg,
        )
        Text(
            text = author,
            fontFamily = InkSans,
            fontWeight = FontWeight.Normal,
            fontSize = maxOf(8f, width.value * 0.085f).sp,
            color = fg.copy(alpha = 0.8f),
        )
    }
}
