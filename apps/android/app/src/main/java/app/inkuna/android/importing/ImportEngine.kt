package app.inkuna.android.importing

import android.content.Context
import android.net.Uri
import android.os.ParcelFileDescriptor
import android.util.Log
import app.inkuna.core.BookshelfInterface
import app.inkuna.core.FdImport
import app.inkuna.core.ImportOutcome
import app.inkuna.core.ImportProgressListener
import app.inkuna.android.model.LibraryStore
import java.io.File
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * Runs EPUB imports and reports them honestly.
 *
 * ## Why this is app-scoped and not a `ViewModel`
 *
 * An import is a write to the shelf, not a screen's private business. It has
 * to survive the screen that started it — a reader who taps back mid-copy
 * should still get the book — so the run lives on [LibraryStore.writes],
 * the application-scoped write scope, and the state it publishes outlives
 * any composition. Being a single object also makes "one import at a time"
 * a real guarantee rather than a per-screen convention: the library screen
 * and an inbound `ACTION_VIEW` share this one engine and this one [state].
 *
 * ## Where the `Bookshelf` comes from
 *
 * From [LibraryStore], never from here. The core requires exactly one
 * `Bookshelf` per data directory for the process lifetime, and `LibraryStore`
 * is the single place that owns it. This engine only ever *receives* it.
 *
 * ## How files reach the core
 *
 * As file descriptors, not paths. SAF hands back a `content://` URI; the
 * provider turns it into a real descriptor, which travels straight to the
 * core — the core's copy into its own storage is the only copy. Staging
 * into cache survives purely as the fallback for virtual documents whose
 * provider can produce a stream but not a descriptor.
 *
 * ## Why the work is chunked
 *
 * For bounded resources, not feedback: a chunk bounds open descriptors
 * (and fallback cache copies) to [CHUNK] books rather than the whole
 * selection, while rayon still parallelizes inside a chunk. Progress is
 * real throughout — `importBatchFds`'s listener reports every finished
 * file while the core works.
 *
 * Nothing here touches the main thread: the core's methods are `suspend` and
 * hop to their own blocking pool, and staging runs on `Dispatchers.IO`.
 */
object ImportEngine {

    private const val TAG = "InkunaImport"

    /**
     * Files opened and handed to `importBatchFds` together. Three keeps
     * rayon busy while bounding what a chunk holds open: three
     * descriptors, and in the fallback case three cache copies.
     */
    private const val CHUNK = 3

    /**
     * How much of one file's progress the fallback staging copy accounts
     * for. The core's pass (hash, dedupe, parse, commit) takes the rest
     * and reports one event per *finished* file, so the bar holds this
     * share until the chunk's first completion event lands. Descriptor
     * imports have no copy phase at all.
     */
    private const val COPY_SHARE = 0.45f

    private val _state = MutableStateFlow<ImportState>(ImportState.Idle)
    val state: StateFlow<ImportState> = _state.asStateFlow()

    @Volatile
    private var runJob: Job? = null

    /** Whether a run is in flight; the entry points disable their trigger on it. */
    val isRunning: Boolean get() = runJob?.isActive == true

    /**
     * Starts importing [uris]. A second call while a run is live is ignored
     * rather than queued — the sheet blocks the trigger, and silently
     * stacking runs would make the progress count lie.
     */
    @Synchronized
    fun start(context: Context, uris: List<Uri>) {
        if (uris.isEmpty()) return
        if (isRunning) return
        val appContext = context.applicationContext
        runJob = LibraryStore.writes.launch { execute(appContext, uris) }
    }

    /** Cancels the live run; the partial report still surfaces. */
    fun cancel() {
        runJob?.cancel()
    }

    /** Clears a finished report, closing the sheet. */
    fun dismiss() {
        if (isRunning) return
        _state.value = ImportState.Idle
    }

    private suspend fun execute(context: Context, uris: List<Uri>) {
        val added = mutableListOf<ImportedBook>()
        val duplicates = mutableListOf<ImportedBook>()
        val failures = mutableListOf<ImportFailure>()
        var cancelled = false
        val total = uris.size
        var completed = 0

        // Names are resolved up front: the provider's grant can lapse the
        // moment the run ends, and every outcome has to be reportable by the
        // name the reader saw in the picker.
        val names = uris.map { ImportStaging.displayName(context, it) }

        // Reading, not Copying: the descriptor route never copies, and the
        // staging callback below flips to Copying for the fallback alone.
        _state.value = ImportState.Running(0, total, names.first(), ImportPhase.Reading, 0f)

        val run = ImportStaging.openRun(context)
        try {
            val shelf: BookshelfInterface = try {
                LibraryStore.bookshelf(context)
            } catch (error: CancellationException) {
                throw error
            } catch (error: Throwable) {
                Log.e(TAG, "The library would not open; nothing can be imported", error)
                names.forEach { failures += ImportFailure.of(it, error) }
                return
            }

            uris.indices.chunked(CHUNK).forEach { chunk ->
                val opened = mutableListOf<OpenDocument>()
                try {
                    chunk.forEach { index ->
                        val name = names[index]
                        try {
                            opened += openDocument(context, uris[index], run, name) { copied, size ->
                                val withinFile = if (size != null && size > 0) {
                                    (copied.toFloat() / size).coerceIn(0f, 1f)
                                } else {
                                    0f
                                }
                                _state.value = ImportState.Running(
                                    completed = completed,
                                    total = total,
                                    currentName = name,
                                    phase = ImportPhase.Copying,
                                    fraction = overall(completed, total, COPY_SHARE * withinFile),
                                )
                            }
                        } catch (error: CancellationException) {
                            throw error
                        } catch (error: Throwable) {
                            // One unreadable pick never costs the rest of the
                            // selection; it is reported by name instead.
                            Log.w(TAG, "Could not open $name", error)
                            failures += ImportFailure.of(name, error)
                            completed++
                        }
                    }

                    if (opened.isEmpty()) return@forEach

                    _state.value = ImportState.Running(
                        completed = completed,
                        total = total,
                        currentName = opened.first().displayName,
                        phase = ImportPhase.Reading,
                        fraction = overall(completed, total, 0f),
                    )

                    // The core fires once per finished file, from its own
                    // worker threads; events arrive with strictly increasing
                    // counts, and StateFlow.value is thread-safe to set. On
                    // the descriptor route `path` is the display name.
                    val base = completed
                    val listener = object : ImportProgressListener {
                        override fun onFileComplete(
                            completed: UInt,
                            total: UInt,
                            path: String,
                        ) {
                            val done = base + completed.toInt()
                            _state.value = ImportState.Running(
                                completed = done,
                                total = uris.size,
                                currentName = path,
                                phase = ImportPhase.Reading,
                                fraction = overall(done, uris.size, 0f),
                            )
                        }
                    }
                    val outcomes = try {
                        // Descriptor ownership transfers with this call;
                        // the core closes every one, success or failure.
                        shelf.importBatchFds(opened.map { it.detachForCore() }, listener)
                    } catch (error: CancellationException) {
                        throw error
                    } catch (error: Throwable) {
                        // importBatchFds throws only when the whole call
                        // fails (the library itself is in trouble), never
                        // per item.
                        Log.e(TAG, "The import batch failed outright", error)
                        opened.forEach { failures += ImportFailure.of(it.displayName, error) }
                        completed += opened.size
                        return@forEach
                    }

                    outcomes.forEach { outcome ->
                        when (outcome) {
                            is ImportOutcome.Imported -> added += ImportedBook.of(outcome.publication)
                            is ImportOutcome.Duplicate -> duplicates += ImportedBook.of(outcome.publication)
                            is ImportOutcome.Failed ->
                                failures += ImportFailure.of(outcome.path, outcome.error)
                        }
                    }
                    completed += opened.size
                } finally {
                    // Undetached descriptors and fallback cache copies never
                    // outlive the chunk, whether it succeeded, failed, or
                    // was cancelled.
                    withContext(NonCancellable) {
                        opened.forEach { it.close() }
                    }
                }
            }
        } catch (_: CancellationException) {
            cancelled = true
        } finally {
            withContext(NonCancellable) {
                run.close()
                _state.value = ImportState.Finished(
                    ImportReport(
                        added = added.toList(),
                        duplicates = duplicates.toList(),
                        failures = failures.toList(),
                        cancelled = cancelled,
                    )
                )
            }
        }
    }

    private fun overall(completed: Int, total: Int, within: Float) =
        ((completed + within) / total).coerceIn(0f, 1f)

    /**
     * Opens one picked document for the core.
     *
     * The direct route asks the provider for a real descriptor, which is
     * all the core needs — no shell-side copy at all. Virtual documents
     * whose provider can only produce a stream fall back to staging into
     * cache first (with byte progress), then travel as a descriptor to
     * that copy.
     */
    private suspend fun openDocument(
        context: Context,
        uri: Uri,
        run: ImportRun,
        name: String,
        onCopyProgress: (copied: Long, total: Long?) -> Unit,
    ): OpenDocument {
        val direct = withContext(Dispatchers.IO) {
            try {
                context.contentResolver.openFileDescriptor(uri, "r")
            } catch (error: CancellationException) {
                throw error
            } catch (error: Throwable) {
                Log.i(TAG, "No direct descriptor for $name; staging instead", error)
                null
            }
        }
        if (direct != null) return OpenDocument(name, direct, staged = null)

        val staged = ImportStaging.stage(context, uri, run, name, onCopyProgress)
        return OpenDocument(
            name,
            ParcelFileDescriptor.open(staged.file, ParcelFileDescriptor.MODE_READ_ONLY),
            staged.file,
        )
    }
}

/**
 * One document opened for the core: a descriptor, plus the cache copy
 * behind it when the provider forced the staging fallback.
 */
private class OpenDocument(
    val displayName: String,
    private var descriptor: ParcelFileDescriptor?,
    private val staged: File?,
) {
    /**
     * Hands the raw descriptor to the core, which owns and closes it from
     * here; [close] then only has the cache copy left to deal with.
     */
    fun detachForCore(): FdImport {
        val pfd = checkNotNull(descriptor) { "descriptor already detached" }
        descriptor = null
        return FdImport(pfd.detachFd(), displayName)
    }

    /** Idempotent: closes an undetached descriptor, deletes the cache copy. */
    fun close() {
        runCatching { descriptor?.close() }
        descriptor = null
        staged?.delete()
    }
}
