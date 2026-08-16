package app.inkuna.android.ui.reader

import android.app.Application
import android.util.Log
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.ViewModelProvider.AndroidViewModelFactory
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import androidx.lifecycle.viewModelScope
import app.inkuna.android.model.LibraryStore
import app.inkuna.core.Bookshelf
import app.inkuna.core.Chapter
import app.inkuna.core.Publication as CorePublication
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import org.json.JSONObject
import org.readium.r2.navigator.epub.EpubNavigatorFactory
import org.readium.r2.shared.publication.Link
import org.readium.r2.shared.publication.Locator
import org.readium.r2.shared.publication.Publication
import org.readium.r2.shared.publication.services.positionsByReadingOrder
import org.readium.r2.shared.util.Url
import org.readium.r2.shared.util.asset.AssetRetriever
import org.readium.r2.shared.util.getOrElse
import org.readium.r2.shared.util.http.DefaultHttpClient
import org.readium.r2.shared.util.toUrl
import org.readium.r2.streamer.PublicationOpener
import org.readium.r2.streamer.parser.DefaultPublicationParser

/**
 * Owns one open book: fetches the core [CorePublication], opens the EPUB at
 * its `filePath` through Readium, and drives the whole core contract —
 * position count, per-page-turn progress, sessions, bookmarks, TOC jumps.
 * Rendering itself belongs to the navigator fragment; storage and progress
 * math belong to the Rust core; this class only ferries between them.
 *
 * Scoped to the reader's back-stack entry, so the opened publication
 * survives configuration changes; only the fragment is rebuilt.
 */
class ReaderViewModel(
    private val app: Application,
    private val publicationId: String,
) : AndroidViewModel(app) {

    sealed interface UiState {
        data object Opening : UiState

        /** Recoverable: the screen offers a retry. */
        data object Failed : UiState

        data class Ready(val book: ReaderBook) : UiState
    }

    /** Everything the reader needs once the book is open. */
    class ReaderBook(
        val core: CorePublication,
        val publication: Publication,
        val navigatorFactory: EpubNavigatorFactory,
        /** The core's saved position, if any — hand it to the navigator. */
        val initialLocator: Locator?,
        /** Readium synthetic position count; 0 until computable. */
        val positionCount: Int,
        val chapters: List<ReaderChapter>,
    )

    /** A core TOC entry resolved against Readium's synthetic positions. */
    data class ReaderChapter(
        val chapter: Chapter,
        /** Index of the chapter's resource in the reading order, if found. */
        val resourceIndex: Int?,
        /** 1-based position where the chapter's resource begins, if known. */
        val position: Int?,
    )

    private val stateFlow = MutableStateFlow<UiState>(UiState.Opening)
    val state: StateFlow<UiState> = stateFlow.asStateFlow()

    private var bookshelf: Bookshelf? = null
    private var openJob: Job? = null

    /**
     * The Readium container, owned from the instant `open` succeeds rather
     * than from the moment the Ready state is published: everything between
     * the two (positions, position count, chapters) can suspend, so a
     * cancellation or a throw there would otherwise leak the open file.
     */
    @Volatile
    private var openPublication: Publication? = null

    // Page turns arrive faster than writes need to land; a StateFlow
    // conflates them so a fast flick persists the settled page, not a queue
    // of intermediate ones. The core expects one `updateProgress` per turn.
    private val pendingProgress = MutableStateFlow<Locator?>(null)
    private var lastPersisted: Locator? = null

    private var sessionId: String? = null

    /**
     * Every core write this reader makes — progress, bookmarks, session
     * start and end — passes through here, in the order it was asked for.
     * Without it the final page turn and the session's closing write race
     * on a multi-threaded scope, and a session closed before the last
     * heartbeat keeps a stale end position (the core only heartbeats
     * sessions with `ended_at IS NULL`).
     */
    private val writeLock = Mutex()

    init {
        open()
        viewModelScope.launch {
            // The emitted value is deliberately ignored: the write always
            // takes the newest pending locator, so a write that waited for
            // the lock can never commit an older page than one that ran.
            pendingProgress.filterNotNull().collect { persistPendingProgress() }
        }
    }

    /** Starts (or after a failure, restarts) opening the book. */
    fun open() {
        if (openJob?.isActive == true || stateFlow.value is UiState.Ready) return
        stateFlow.value = UiState.Opening
        openJob = viewModelScope.launch {
            // A retry must not leave the failed attempt's container open.
            closeOpenPublication()
            stateFlow.value = try {
                UiState.Ready(doOpen())
            } catch (e: Exception) {
                Log.w(TAG, "opening $publicationId failed", e)
                // Cancellation lands here too, and then no suspending close
                // would run — hand it to the write scope, which outlives us.
                LibraryStore.writes.launch { closeOpenPublication() }
                UiState.Failed
            }
        }
    }

    private suspend fun doOpen(): ReaderBook {
        val shelf = LibraryStore.bookshelf(app)
        bookshelf = shelf
        val core = shelf.publication(publicationId)

        val httpClient = DefaultHttpClient()
        val assetRetriever = AssetRetriever(app.contentResolver, httpClient)
        val asset = assetRetriever
            .retrieve(File(core.filePath).toUrl(isDirectory = false))
            .getOrElse { error -> throw ReaderOpenException(error.message) }
        val opener = PublicationOpener(
            publicationParser = DefaultPublicationParser(
                app,
                httpClient = httpClient,
                assetRetriever = assetRetriever,
                pdfFactory = null,
            ),
        )
        val publication = opener
            .open(asset, allowUserInteraction = false)
            .getOrElse { error ->
                asset.close()
                throw ReaderOpenException(error.message)
            }
        // Owned from here on, whatever the rest of this function does.
        openPublication = publication

        // Synthetic positions are the honest substitute for page numbers.
        // Reported once so the core can answer "p. N of M" everywhere.
        val positionsByResource = publication.positionsByReadingOrder()
        val positionCount = positionsByResource.sumOf { it.size }
        if (positionCount > 0 && core.positionCount != positionCount.toUInt()) {
            shelf.reportPositionCount(core.id, positionCount.toUInt())
        }

        val chapters = shelf.chapters(core.id).map { chapter ->
            // The chapter-to-resource mapping is href-minus-fragment, per
            // the core spec; the reading-order index it yields is what both
            // the position and the "you are here" highlight are built on.
            val resourceIndex = Url(chapter.href)?.let { readingOrderIndex(publication, it) }
            ReaderChapter(
                chapter = chapter,
                resourceIndex = resourceIndex,
                position = resourceIndex
                    ?.let { positionsByResource.getOrNull(it) }
                    ?.firstOrNull()
                    ?.locations
                    ?.position,
            )
        }

        // The locator blob is opaque to the core; only Readium parses it.
        // A blob this navigator cannot read (corrupt, or from a future
        // format) degrades to opening at the start, never to a crash.
        val initialLocator = core.locator?.let { raw ->
            runCatching { Locator.fromJSON(JSONObject(raw)) }.getOrNull()
        }

        return ReaderBook(
            core = core,
            publication = publication,
            navigatorFactory = EpubNavigatorFactory(publication),
            initialLocator = initialLocator,
            positionCount = positionCount,
            chapters = chapters,
        )
    }

    /** Where [href] sits in the reading order, ignoring any fragment. */
    private fun readingOrderIndex(publication: Publication, href: Url): Int? {
        val target = href.removeFragment().normalize()
        return publication.readingOrder
            .indexOfFirst { link -> link.url().normalize().removeFragment() == target }
            .takeIf { it >= 0 }
    }

    /**
     * The TOC entry to mark as "you are here" for [locator].
     *
     * Highlights the last chapter whose resource starts at or before the
     * current resource, resolving a tie — several chapters sharing one
     * XHTML file, which is how many EPUBs are built — to the first of them.
     * A resource with no TOC entry of its own therefore keeps the preceding
     * chapter lit rather than clearing the highlight. Matching is on
     * reading-order indices, never on href strings, so an entry whose href
     * merely spells the current one differently still counts.
     */
    fun currentChapterIndex(locator: Locator?): Int? {
        val book = (stateFlow.value as? UiState.Ready)?.book ?: return null
        val here = locator?.let { readingOrderIndex(book.publication, it.href) } ?: return null
        val chapterResource = book.chapters
            .mapNotNull { it.resourceIndex }
            .filter { it <= here }
            .maxOrNull() ?: return null
        return book.chapters.indexOfFirst { it.resourceIndex == chapterResource }.takeIf { it >= 0 }
    }

    /** Resolves a core chapter's href into a navigator jump target. */
    fun chapterLocator(chapter: Chapter): Locator? {
        val book = (stateFlow.value as? UiState.Ready)?.book ?: return null
        val url = Url(chapter.href) ?: return null
        return book.publication.locatorFromLink(Link(href = url))
    }

    /** One call per page turn, from the navigator's locator flow. */
    fun onLocatorChanged(locator: Locator) {
        pendingProgress.value = locator
    }

    /**
     * Writes the newest unpersisted page position, if there is one.
     *
     * The pending locator is read *inside* the lock rather than passed in:
     * a caller that waited on the lock would otherwise commit whatever page
     * it captured before waiting, letting an older locator land last.
     */
    private suspend fun persistPendingProgress() {
        val shelf = bookshelf ?: return
        withContext(NonCancellable + Dispatchers.Default) {
            writeLock.withLock {
                val locator = pendingProgress.value ?: return@withLock
                if (locator === lastPersisted) return@withLock
                // The book-wide totalProgression, never the per-resource one.
                val progression = locator.locations.totalProgression ?: return@withLock
                lastPersisted = locator
                runCatching {
                    shelf.updateProgress(
                        publicationId,
                        locator.toJSON().toString(),
                        progression,
                        locator.locations.position?.toUInt(),
                    )
                }.onFailure { Log.w(TAG, "updateProgress failed", it) }
            }
        }
    }

    /**
     * Reading sessions bracket the reader's visible lifetime — entered /
     * left / backgrounded — and power the Stats screen. Writes run on the
     * application scope so popping the reader never cancels the closing
     * write; a session lost to a crash is closed retroactively by the
     * core at the next `sessionStart`.
     */
    fun onReaderVisible() {
        LibraryStore.writes.launch {
            withContext(NonCancellable) {
                writeLock.withLock {
                    if (sessionId != null) return@withLock
                    val shelf = bookshelf ?: return@withLock
                    sessionId = runCatching { shelf.sessionStart(publicationId) }
                        .onFailure { Log.w(TAG, "sessionStart failed", it) }
                        .getOrNull()
                }
            }
        }
    }

    fun onReaderHidden() {
        endSitting(closePublication = false)
    }

    /**
     * The one teardown coroutine: the final page position lands first, then
     * the session closes around it, and — when the reader is going away for
     * good — the container is released. One coroutine, so the order is the
     * order of these lines rather than of whichever thread woke first.
     */
    private fun endSitting(closePublication: Boolean) {
        LibraryStore.writes.launch {
            persistPendingProgress()
            endSession()
            if (closePublication) closeOpenPublication()
        }
    }

    private suspend fun endSession() {
        withContext(NonCancellable) {
            writeLock.withLock {
                val id = sessionId ?: return@withLock
                sessionId = null
                val shelf = bookshelf ?: return@withLock
                runCatching { shelf.sessionEnd(id) }
                    .onFailure { Log.w(TAG, "sessionEnd failed", it) }
            }
        }
    }

    /** `Publication.close()` blocks, so it never runs on the main thread. */
    private suspend fun closeOpenPublication() {
        val publication = openPublication ?: return
        openPublication = null
        withContext(NonCancellable + Dispatchers.IO) {
            runCatching { publication.close() }
                .onFailure { Log.w(TAG, "closing the publication failed", it) }
        }
    }

    /**
     * Persists a bookmark at [locator]; [onPlaced] confirms on success.
     *
     * On the application write scope, like the session writes above: leaving
     * the book the instant after the tap must not cancel the write. The
     * confirmation ([onPlaced] drives haptics and the toast) is dispatched
     * back to the main thread.
     */
    fun addBookmark(locator: Locator, onPlaced: () -> Unit) {
        val shelf = bookshelf ?: return
        LibraryStore.writes.launch {
            writeLock.withLock {
                runCatching {
                    shelf.addBookmark(
                        publicationId,
                        locator.toJSON().toString(),
                        locator.locations.totalProgression ?: 0.0,
                    )
                }.onSuccess { withContext(Dispatchers.Main) { onPlaced() } }
                    .onFailure { Log.w(TAG, "addBookmark failed", it) }
            }
        }
    }

    override fun onCleared() {
        // The last page turn may still sit unconsumed in the conflated flow.
        // It flushes ahead of the session's closing write, and the container
        // closes only once both have landed.
        endSitting(closePublication = true)
        super.onCleared()
    }

    private class ReaderOpenException(message: String) : Exception(message)

    companion object {
        private const val TAG = "InkunaReader"

        fun factory(publicationId: String) = viewModelFactory {
            initializer {
                val application = this[AndroidViewModelFactory.APPLICATION_KEY]!!
                ReaderViewModel(application, publicationId)
            }
        }
    }
}
