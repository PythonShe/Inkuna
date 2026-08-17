package app.inkuna.android.ui.components

import android.graphics.BitmapFactory
import android.util.LruCache
import androidx.compose.runtime.Composable
import androidx.compose.runtime.key
import androidx.compose.runtime.produceState
import androidx.compose.runtime.State
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * Decoded covers, keyed by path and target width. Tab switches drop the
 * unselected screen from composition entirely, so without a process-level
 * cache every return to a tab restarted the decode from disk and the art
 * blinked back in over the placeholder. Sized in bytes: an eighth of the
 * heap comfortably holds every thumbnail a large library scrolls through.
 */
private val coverCache = object : LruCache<String, ImageBitmap>(
    (Runtime.getRuntime().maxMemory() / 8).toInt().coerceAtMost(64 * 1024 * 1024),
) {
    override fun sizeOf(key: String, value: ImageBitmap) = value.width * value.height * 4
}

/**
 * Decodes the core-extracted cover at [path] off the main thread, sampled
 * down to roughly the tile's pixel width so a full-resolution cover never
 * inflates a 52dp thumbnail. A previously decoded cover is served
 * synchronously from the cache, so it is present on the very first frame;
 * only a genuinely new decode starts at null. Stays null on a missing or
 * corrupt file, which keeps the generated cover standing.
 */
@Composable
fun rememberCoverArt(path: String?, width: Dp): State<ImageBitmap?> {
    val targetWidthPx = with(LocalDensity.current) { width.roundToPx() }
    val cacheKey = if (path == null) null else "$path#$targetWidthPx"
    // key() gives the produced state the same identity as its inputs. The
    // lists here reuse slots positionally, so a slot handed a different
    // book must start from that book's cached art (or null) — without the
    // key, the state keeps the previous book's bitmap and the guard below
    // would then pin the wrong cover forever.
    return key(cacheKey) {
        val cached = cacheKey?.let(coverCache::get)
        produceState(initialValue = cached, cacheKey) {
            if (cacheKey == null || path == null || value != null) return@produceState
            val decoded = withContext(Dispatchers.IO) { decodeSampled(path, targetWidthPx) }
            if (decoded != null) coverCache.put(cacheKey, decoded)
            value = decoded
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
