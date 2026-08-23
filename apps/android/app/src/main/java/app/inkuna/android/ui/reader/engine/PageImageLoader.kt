package app.inkuna.android.ui.reader.engine

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
    private val scope: CoroutineScope,
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

        scope.launch(Dispatchers.Default) {
            val bitmap = runCatching {
                val bytes = session.resource(href)
                BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
            }.getOrNull()
            withContext(Dispatchers.Main.immediate) {
                finish(href, bitmap)
            }
        }
        return null
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

    private companion object {
        const val CACHE_BYTES = 32 * 1024 * 1024
    }
}
