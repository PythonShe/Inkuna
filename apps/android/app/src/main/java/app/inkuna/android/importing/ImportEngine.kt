package app.inkuna.android.importing

import android.content.Context
import android.net.Uri
import android.util.Log
import app.inkuna.core.BookshelfInterface
import app.inkuna.core.ImportOutcome
import app.inkuna.core.ImportProgressListener
import app.inkuna.android.model.LibraryStore
import kotlinx.coroutines.CancellationException
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
 * ## Why the work is chunked
 *
 * For disk, not feedback: staging each chunk's files into cache bounds
 * cache use to [CHUNK] books rather than the whole selection, while rayon
 * still parallelizes inside a chunk. Progress is real throughout — the
 * copy reports bytes, and `importBatch`'s listener reports every finished
 * file while the core works.
 *
 * Nothing here touches the main thread: the core's methods are `suspend` and
 * hop to their own blocking pool, and staging runs on `Dispatchers.IO`.
 */
object ImportEngine {

    private const val TAG = "InkunaImport"

    /**
     * Files staged and handed to `importBatch` together. Three keeps rayon
     * busy without letting three copies of a large book — plus the core's
     * own staging copy — sit in cache at once.
     */
    private const val CHUNK = 3

    /**
     * How much of one file's progress the copy accounts for. The core's
     * pass (hash, dedupe, parse, commit) takes the rest and reports one
     * event per *finished* file, so the bar holds this share until the
     * chunk's first completion event lands.
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

        _state.value = ImportState.Running(0, total, names.first(), ImportPhase.Copying, 0f)

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
                val staged = mutableListOf<StagedDocument>()
                try {
                    chunk.forEach { index ->
                        val name = names[index]
                        try {
                            staged += ImportStaging.stage(context, uris[index], run, name) { copied, size ->
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
                            Log.w(TAG, "Could not stage $name", error)
                            failures += ImportFailure.of(name, error)
                            completed++
                        }
                    }

                    if (staged.isEmpty()) return@forEach

                    _state.value = ImportState.Running(
                        completed = completed,
                        total = total,
                        currentName = staged.first().displayName,
                        phase = ImportPhase.Reading,
                        // Hold the copy's share until the core's first
                        // per-file completion event moves the bar for real.
                        fraction = overall(completed, total, COPY_SHARE),
                    )

                    val byPath = staged.associateBy { it.path }
                    // The core fires once per finished file, from its own
                    // worker threads; events arrive with strictly increasing
                    // counts, and StateFlow.value is thread-safe to set.
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
                                currentName = byPath[path]?.displayName
                                    ?: path.substringAfterLast('/'),
                                phase = ImportPhase.Reading,
                                fraction = overall(done, uris.size, 0f),
                            )
                        }
                    }
                    val outcomes = try {
                        shelf.importBatch(staged.map { it.path }, listener)
                    } catch (error: CancellationException) {
                        throw error
                    } catch (error: Throwable) {
                        // importBatch throws only when the whole call fails
                        // (the library itself is in trouble), never per item.
                        Log.e(TAG, "The import batch failed outright", error)
                        staged.forEach { failures += ImportFailure.of(it.displayName, error) }
                        completed += staged.size
                        return@forEach
                    }

                    outcomes.forEach { outcome ->
                        when (outcome) {
                            is ImportOutcome.Imported -> added += ImportedBook.of(outcome.publication)
                            is ImportOutcome.Duplicate -> duplicates += ImportedBook.of(outcome.publication)
                            is ImportOutcome.Failed -> {
                                val name = byPath[outcome.path]?.displayName
                                    ?: outcome.path.substringAfterLast('/')
                                failures += ImportFailure.of(name, outcome.error)
                            }
                        }
                    }
                    completed += staged.size
                } finally {
                    // Cache copies never outlive the chunk that made them,
                    // whether it succeeded, failed, or was cancelled.
                    withContext(NonCancellable) {
                        staged.forEach { it.file.delete() }
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
}
