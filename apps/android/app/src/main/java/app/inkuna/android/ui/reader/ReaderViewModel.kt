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
import java.util.concurrent.atomic.AtomicReference
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
    /** A chapter href to open at instead of the saved position (a
     *  detail-screen contents row); null resumes where the reader left off. */
    private val initialChapterHref: String? = null,
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
        /** 1-based position each reading-order resource starts at, by
         *  reading-order index; empty when positions are uncomputable. */
        val resourceStarts: List<Int>,
        /** How many synthetic positions each resource holds, same order. */
        val resourceCounts: List<Int>,
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

    // @Volatile: assigned on the main thread when the book opens, read from
    // the application write scope's worker threads by the progress and
    // session writes below.
    @Volatile
    private var bookshelf: Bookshelf? = null
    private var openJob: Job? = null

    /**
     * The Readium container, owned from the instant `open` succeeds rather
     * than from the moment the Ready state is published: everything between
     * the two (positions, position count, chapters) can suspend, so a
     * cancellation or a throw there would otherwise leak the open file.
     * An atomic get-and-set hands the instance to exactly one closer, and
     * the instance is captured at scheduling time — a close queued for a
     * failed attempt can never reach the container a retry opened after it.
     */
    private val openPublication = AtomicReference<Publication?>(null)

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

    /**
     * Tail of the FIFO chain every lifecycle-ordered write joins. The lock
     * above makes writes atomic; this chain makes them *ordered*: a
     * `sessionStart` asked for after a teardown must run after the whole
     * teardown, or a rotation's stop→start round trip lets the queued
     * `sessionEnd` close the session the restart just opened, silencing
     * every heartbeat for the rest of the sitting. Only touched from the
     * main thread (lifecycle callbacks and `onCleared`), so plain field
     * handover is safe; the same shape as the iOS reader's write chain.
     */
    private var writeTail: Job? = null

    private fun enqueueCoreWrite(block: suspend () -> Unit): Job {
        val previous = writeTail
        val job = LibraryStore.writes.launch {
            previous?.join()
            block()
        }
        writeTail = job
        return job
    }

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
            // The snapshot happens here, synchronously, so the queued close
            // can only ever touch the attempt that failed.
            closeOpenPublicationAsync()
            stateFlow.value = try {
                UiState.Ready(doOpen())
            } catch (e: Exception) {
                Log.w(TAG, "opening $publicationId failed", e)
                // Cancellation lands here too, and then no suspending close
                // would run — hand it to the write scope, which outlives us.
                closeOpenPublicationAsync()
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
        openPublication.set(publication)

        // Synthetic positions are the honest substitute for page numbers.
        // Reported once so the core can answer "p. N of M" everywhere.
        val positionsByResource = publication.positionsByReadingOrder()
        val positionCount = positionsByResource.sumOf { it.size }
        // Per-resource counts, not just the total: the core derives each
        // chapter's position range from them, which is what "pages left
        // in this chapter" and a search hit's "p. N" are built on.
        val positionRanges = if (positionCount > 0) {
            positionsByResource.map { it.size.toUInt() }
        } else {
            emptyList()
        }
        runCatching {
            shelf.reportPositionRanges(core.id, positionRanges)
        }.onFailure { error ->
            // The core rejects a breakdown that does not line up with its
            // own spine — it drops duplicate and over-long spine hrefs that
            // Readium's reading order keeps. The per-chapter spans are
            // lost, but "p. N of M" need not be.
            Log.w(TAG, "reportPositionRanges failed", error)
            if (positionCount > 0) {
                runCatching {
                    shelf.reportPositionCount(core.id, positionCount.toUInt())
                }.onFailure { Log.w(TAG, "reportPositionCount failed", it) }
            }
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

        // A requested start chapter wins over the saved position, resolved
        // the same way a contents-sheet jump is; an unresolvable href falls
        // back to resuming. The locator blob is opaque to the core; only
        // Readium parses it. A blob this navigator cannot read (corrupt, or
        // from a future format) degrades to opening at the start, never to
        // a crash.
        val chapterTarget = initialChapterHref
            ?.let { href -> Url(href) }
            ?.let { url -> publication.locatorFromLink(Link(href = url)) }
        val initialLocator = chapterTarget ?: core.locator?.let { raw ->
            runCatching { Locator.fromJSON(JSONObject(raw)) }.getOrNull()
        }

        return ReaderBook(
            core = core,
            publication = publication,
            navigatorFactory = EpubNavigatorFactory(publication),
            initialLocator = initialLocator,
            positionCount = positionCount,
            chapters = chapters,
            resourceStarts = positionsByResource.map {
                it.firstOrNull()?.locations?.position ?: 0
            },
            resourceCounts = positionsByResource.map { it.size },
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

    /**
     * One in-book search hit, projected stable for Compose off the core's
     * record (UniFFI records are `var`-fielded and recompose on every read).
     */
    data class SearchHit(
        /** Reading-order index of the resource the hit sits in. */
        val spineIndex: Int,
        /** Package-relative href of that resource. */
        val href: String,
        /** Char offset of the match — with [spineIndex], a stable key. */
        val charOffset: Long,
        val snippetPre: String,
        val snippetMatch: String,
        val snippetPost: String,
        /** In-resource progression, 0..1: the jump target's precision. */
        val progression: Double,
        /** Readium synthetic position of the hit, when computable. */
        val position: Int?,
    )

    /** What one search answered: hits, the true total, and whether it ran. */
    data class SearchOutcome(
        val hits: List<SearchHit> = emptyList(),
        /** The core's total, which may exceed [hits] — the cap is ours. */
        val total: Int = 0,
        /** The index is derived data; a failure means "not right now". */
        val unavailable: Boolean = false,
    )

    /**
     * In-book search through the core's case-folded, CJK-aware index.
     *
     * Suspends on the caller's coroutine, so a debounced caller cancelling
     * a keystroke cancels the query with it. Positions are resolved here,
     * against the same Readium synthetic positions the footer counts in.
     */
    suspend fun search(query: String): SearchOutcome {
        val book = (stateFlow.value as? UiState.Ready)?.book ?: return SearchOutcome()
        val shelf = bookshelf ?: return SearchOutcome()
        val results = try {
            shelf.searchInBook(publicationId, query, SEARCH_LIMIT)
        } catch (cancellation: kotlinx.coroutines.CancellationException) {
            throw cancellation
        } catch (failure: Throwable) {
            Log.w(TAG, "searchInBook failed", failure)
            return SearchOutcome(unavailable = true)
        }
        return SearchOutcome(
            hits = results.hits.map { hit ->
                // The href is authoritative; the core's spine index is the
                // fallback for a resource this navigator spells differently.
                val resource = Url(hit.href)
                    ?.let { readingOrderIndex(book.publication, it) }
                    ?: hit.spineIdx.toInt()
                SearchHit(
                    spineIndex = resource,
                    href = hit.href,
                    charOffset = hit.charOffset.toLong(),
                    snippetPre = hit.snippetPre,
                    snippetMatch = hit.snippetMatch,
                    snippetPost = hit.snippetPost,
                    progression = hit.progression,
                    position = positionOf(book, resource, hit.progression),
                )
            },
            total = results.total.toInt(),
        )
    }

    /**
     * Where a hit falls in Readium's synthetic positions: the resource's
     * first position plus the whole positions its progression covers,
     * clamped inside the resource. Null when positions are unknown — an
     * honest blank beats an invented page.
     */
    private fun positionOf(book: ReaderBook, resource: Int, progression: Double): Int? {
        val start = book.resourceStarts.getOrNull(resource) ?: return null
        val count = book.resourceCounts.getOrNull(resource) ?: return null
        if (start <= 0 || count <= 0) return null
        val within = (progression.coerceIn(0.0, 1.0) * count).toInt().coerceIn(0, count - 1)
        return start + within
    }

    /** The navigator jump target for a search hit. */
    fun searchLocator(hit: SearchHit): Locator? {
        val book = (stateFlow.value as? UiState.Ready)?.book ?: return null
        val url = Url(hit.href) ?: return null
        val locator = book.publication.locatorFromLink(Link(href = url)) ?: return null
        return locator.copyWithLocations(
            progression = hit.progression,
            position = hit.position,
        )
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
        enqueueCoreWrite {
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
        enqueueCoreWrite {
            persistPendingProgress()
            endSession()
            if (closePublication) {
                openPublication.getAndSet(null)?.let { close(it) }
            }
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

    /**
     * Snapshots and releases the current container on the calling thread,
     * then closes that exact instance off it. Capturing the reference
     * before anything else can run is what keeps a close scheduled for a
     * dead attempt away from the container a retry opens next.
     */
    private fun closeOpenPublicationAsync() {
        val publication = openPublication.getAndSet(null) ?: return
        LibraryStore.writes.launch { close(publication) }
    }

    /** `Publication.close()` blocks, so it never runs on the main thread. */
    private suspend fun close(publication: Publication) {
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
        enqueueCoreWrite {
            // The main-thread confirmation hop stays outside the lock: held
            // across it, every other reader write would queue behind main-
            // thread availability.
            writeLock.withLock {
                runCatching {
                    shelf.addBookmark(
                        publicationId,
                        locator.toJSON().toString(),
                        locator.locations.totalProgression ?: 0.0,
                    )
                }
            }
                .onSuccess { withContext(Dispatchers.Main) { onPlaced() } }
                .onFailure { Log.w(TAG, "addBookmark failed", it) }
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

        /** More hits than a panel can be scrolled through in one sitting;
         *  the true total is reported separately. */
        private const val SEARCH_LIMIT = 200u

        fun factory(publicationId: String, initialChapterHref: String? = null) = viewModelFactory {
            initializer {
                val application = this[AndroidViewModelFactory.APPLICATION_KEY]!!
                ReaderViewModel(application, publicationId, initialChapterHref)
            }
        }
    }
}
