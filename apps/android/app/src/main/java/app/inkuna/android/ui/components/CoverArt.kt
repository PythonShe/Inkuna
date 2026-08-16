package app.inkuna.android.ui.components

import android.graphics.BitmapFactory
import androidx.compose.runtime.Composable
import androidx.compose.runtime.produceState
import androidx.compose.runtime.State
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * Decodes the core-extracted cover at [path] off the main thread, sampled
 * down to roughly the tile's pixel width so a full-resolution cover never
 * inflates a 52dp thumbnail. Yields null until decoded — and stays null on
 * a missing or corrupt file, which keeps the generated cover standing.
 */
@Composable
fun rememberCoverArt(path: String?, width: Dp): State<ImageBitmap?> {
    val targetWidthPx = with(LocalDensity.current) { width.roundToPx() }
    return produceState<ImageBitmap?>(initialValue = null, path, targetWidthPx) {
        value = if (path == null) {
            null
        } else {
            withContext(Dispatchers.IO) { decodeSampled(path, targetWidthPx) }
        }
    }
}

private fun decodeSampled(path: String, targetWidthPx: Int): ImageBitmap? {
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeFile(path, bounds)
    if (bounds.outWidth <= 0 || bounds.outHeight <= 0) return null

    val options = BitmapFactory.Options().apply {
        inSampleSize = sampleSize(
            bounds.outWidth,
            bounds.outHeight,
            targetWidthPx,
            // The tile is 2:3, so the height budget is the width's 1.5×; a
            // pathologically tall cover must be sampled by its height too or
            // it allocates its full pixel count at inSampleSize 1.
            targetWidthPx * 3 / 2,
        )
    }
    return BitmapFactory.decodeFile(path, options)?.asImageBitmap()
}

/** Largest power of two keeping both decoded axes at or above their target. */
private fun sampleSize(
    sourceWidth: Int,
    sourceHeight: Int,
    targetWidthPx: Int,
    targetHeightPx: Int,
): Int {
    if (targetWidthPx <= 0 || targetHeightPx <= 0) return 1
    var sample = 1
    while (
        sourceWidth / (sample * 2) >= targetWidthPx ||
        sourceHeight / (sample * 2) >= targetHeightPx
    ) {
        sample *= 2
    }
    return sample
}
