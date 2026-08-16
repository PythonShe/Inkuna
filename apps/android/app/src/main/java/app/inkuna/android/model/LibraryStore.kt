package app.inkuna.android.model

import android.content.Context
import android.util.Log
import app.inkuna.core.Bookshelf
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
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
            opened ?: withContext(Dispatchers.IO) {
                Bookshelf.open(dataDir.absolutePath)
            }.also { shelf ->
                opened = shelf
                // Covers imported by older cores are full-resolution
                // originals; normalize them into the core's bounded WebP
                // form off the critical path. Idempotent and cheap when
                // there is nothing to do; failing only means covers stay
                // big until the next open.
                writes.launch {
                    try {
                        shelf.optimizeCovers()
                    } catch (cancellation: CancellationException) {
                        throw cancellation
                    } catch (failure: Throwable) {
                        Log.w(TAG, "Cover optimization failed", failure)
                    }
                }
            }
        }
    }

    private const val TAG = "LibraryStore"
}
