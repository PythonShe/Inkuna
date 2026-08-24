package app.inkuna.android.model

import android.content.Context
import android.util.Log
import app.inkuna.core.Bookshelf
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

/**
 * App-side owner of the core [Bookshelf] — the Android mirror of iOS's
 * `LibraryStore` actor.
 *
 * Opening is not cheap: the core runs its schema migrations and then sweeps
 * files no row references, both synchronously — so it always happens on the
 * IO dispatcher, never the main thread. The core also requires exactly one
 * [Bookshelf] per data directory for the process lifetime — a second one
 * would sweep the first one's in-flight import — which the cached instance
 * is what enforces.
 *
 * Opening can fail — a database damaged by a device-full write, a directory
 * the app cannot create — and that is thrown to the caller rather than
 * trapped: a reader whose library will not open needs a screen they can
 * retry from, not an app that cannot launch. Nothing is cached on failure,
 * so a later call retries.
 */
object LibraryStore {

    /**
     * Fire-and-forget core writes that must outlive the screen that issued
     * them — a session ending as the reader is popped, the final progress
     * write of a sitting. Application-scoped so a screen's disposal never
     * cancels a write mid-flight.
     */
    val writes: CoroutineScope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    private val openLock = Mutex()

    @Volatile
    private var opened: Bookshelf? = null

    /** The core library, opened on first use. */
    suspend fun bookshelf(context: Context): Bookshelf {
        opened?.let { return it }
        // The core owns everything under this directory: inkuna.db, books/,
        // and covers/. `filesDir` survives backups and never needs runtime
        // permissions.
        val dataDir = context.applicationContext.filesDir
        return openLock.withLock {
            // Once `Bookshelf.open` returns, `opened` MUST be set: a
            // caller's cancellation between the two would orphan the live
            // shelf, and the next call would construct a second Bookshelf
            // on the same data directory — forbidden, it would sweep the
            // first's in-flight import. NonCancellable makes the whole
            // open-and-cache block immune to the caller's cancellation,
            // which resurfaces at the caller's next suspension point.
            opened ?: withContext(NonCancellable + Dispatchers.IO) {
                // The engine shapes with the bundled font set and needs a
                // real directory to read it from, so the APK's copy is
                // unpacked first. A failure here fails the open, which the
                // caller's retry path already covers — a reader engine with
                // no fonts could not lay a page out anyway.
                val shelf = Bookshelf.open(
                    dataDir.absolutePath,
                    CoreFonts.ensureExtracted(context).absolutePath,
                )
                // Hand the platform faces (Noto Serif / Roboto) to the
                // engine before the shelf is cached and anyone can start a
                // reader session — the core accepts this call exactly once
                // and only before the first session. A failure — discovery
                // included — never fails the open: the engine simply falls
                // back to the bundled Notos for the system-font choices.
                // (No CancellationException rethrow: this block is
                // NonCancellable, so one here is a genuine failure that
                // must not skip caching the shelf below.)
                try {
                    val faces = SystemReadingFonts.discover()
                    if (faces.isNotEmpty()) {
                        shelf.registerSystemFonts(faces).forEach { warning ->
                            Log.w(TAG, "System font skipped (${warning.filePath}): ${warning.detail}")
                        }
                    }
                } catch (failure: Throwable) {
                    Log.w(TAG, "System font registration failed", failure)
                }
                // The cache-store stays inside the NonCancellable block:
                // the dispatch back to a cancelled caller can throw before
                // any code after `withContext` runs.
                opened = shelf
                // Covers imported by older cores are full-resolution
                // originals; normalize them into the core's bounded WebP
                // form off the critical path. Idempotent and cheap when
                // there is nothing to do; failing only means covers stay
                // big until the next open.
                writes.launch {
                    try {
                        shelf.importer().optimizeCovers()
                    } catch (cancellation: CancellationException) {
                        throw cancellation
                    } catch (failure: Throwable) {
                        Log.w(TAG, "Cover optimization failed", failure)
                    }
                }
                shelf
            }
        }
    }

    private const val TAG = "LibraryStore"
}
