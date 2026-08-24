package app.inkuna.android.ui.reader.engine

import android.content.res.Resources
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.util.LruCache
import app.inkuna.core.ReaderSession
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** Caches decoded page images and shares one resource request per href. */
class PageImageLoader(
    private val session: ReaderSession,
    private val scope: () -> CoroutineScope,
) {
    private val cache = object : LruCache<String, Bitmap>(CACHE_BYTES) {
        override fun sizeOf(key: String, value: Bitmap): Int = value.allocationByteCount
    }
    private val inFlight = mutableSetOf<String>()
    private val callbacks = mutableMapOf<String, MutableList<() -> Unit>>()
    private val permanentlyMissing = mutableSetOf<String>()

    fun image(href: String, onReady: () -> Unit): Bitmap? {
        synchronized(this) {
            cache.get(href)?.let { return it }
            if (href in permanentlyMissing) return null
            callbacks.getOrPut(href) { mutableListOf() } += onReady
            if (!inFlight.add(href)) return null
        }

        scope().launch(Dispatchers.Default) {
            val bitmap = runCatching {
                decodeCapped(session.resource(href))
            }.getOrNull()
            withContext(Dispatchers.Main.immediate) {
                finish(href, bitmap)
            }
        }.invokeOnCompletion { cause ->
            if (cause != null) abandon(href)
        }
        return null
    }

    private fun abandon(href: String) {
        synchronized(this) {
            inFlight.remove(href)
            callbacks.remove(href)
        }
    }

    private fun finish(href: String, bitmap: Bitmap?) {
        val ready: List<() -> Unit>
        synchronized(this) {
            inFlight.remove(href)
            if (bitmap == null) {
                permanentlyMissing += href
                callbacks.remove(href)
                return
            }
            cache.put(href, bitmap)
            ready = callbacks.remove(href).orEmpty()
        }
        ready.forEach { it() }
    }

    /**
     * Decodes with the dimensions read first, downsampling until BOTH the
     * edge cap and the total-pixel-area cap hold, before any pixel
     * allocation — a pathological 16000x16000 source must never
     * materialize at full size ahead of the LruCache's byte budget, and a
     * near-square bitmap within the edge cap alone could still allocate
     * several times the area the display can show (the same two-cap
     * contract the iOS shell enforces). Anything still over a cap at the
     * maximum sample factor is rejected.
     */
    private fun decodeCapped(bytes: ByteArray): Bitmap? {
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
        val width = bounds.outWidth
        val height = bounds.outHeight
        if (width <= 0 || height <= 0) return null
        var sample = 1
        while (true) {
            val sampledWidth = (width + sample - 1) / sample
            val sampledHeight = (height + sample - 1) / sample
            if (maxOf(sampledWidth, sampledHeight) <= maxEdgePx &&
                sampledWidth.toLong() * sampledHeight.toLong() <= maxAreaPx
            ) {
                break
            }
            if (sample >= MAX_SAMPLE) return null
            sample *= 2
        }
        val options = BitmapFactory.Options().apply { inSampleSize = sample }
        return BitmapFactory.decodeByteArray(bytes, 0, bytes.size, options)
    }

    private companion object {
        const val CACHE_BYTES = 32 * 1024 * 1024

        /** No page image needs more than twice the display's longest edge. */
        val maxEdgePx: Int = 2 * maxOf(
            Resources.getSystem().displayMetrics.widthPixels,
            Resources.getSystem().displayMetrics.heightPixels,
            1024,
        )

        /**
         * Nor more total pixels than (2x display width) x (2x display
         * height) — each dimension floored at 1024, so degenerate metrics
         * still yield a 2048x2048 area floor, matching the iOS shell.
         */
        val maxAreaPx: Long =
            2L * maxOf(Resources.getSystem().displayMetrics.widthPixels, 1024) *
                2L * maxOf(Resources.getSystem().displayMetrics.heightPixels, 1024)
        const val MAX_SAMPLE = 32
    }
}
