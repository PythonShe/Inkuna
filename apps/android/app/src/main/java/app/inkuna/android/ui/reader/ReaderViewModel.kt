package app.inkuna.android.ui.reader

import android.app.Application
import android.os.SystemClock
import android.util.Log
import android.view.WindowInsets
import android.view.WindowManager
import androidx.compose.ui.unit.dp
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.ViewModelProvider.AndroidViewModelFactory
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import androidx.lifecycle.viewModelScope
import app.inkuna.android.model.AppSettings
import app.inkuna.android.model.LibraryStore
import app.inkuna.android.ui.ReaderPerf
import app.inkuna.core.BookSearchHit
import app.inkuna.core.Bookmark
import app.inkuna.core.Chapter
import app.inkuna.core.ChapterPositionRange
import app.inkuna.core.Coordinate
import app.inkuna.core.InkunaException
import app.inkuna.core.LayoutListener
import app.inkuna.core.PageLocation
import app.inkuna.core.Publication
import app.inkuna.core.ReaderLayoutSettings
import app.inkuna.core.ReaderSession
import app.inkuna.core.Viewport
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlin.math.roundToLong

/** One engine-backed reader; synchronous session reads are deliberately cache-only. */
class ReaderViewModel(
    private val app: Application,
    private val publicationId: String,
    private val initialChapterHref: String? = null,
) : AndroidViewModel(app) {

    sealed interface UiState {
        data object Opening : UiState
        data object Failed : UiState
        data object FixedLayoutUnsupported : UiState
        data class Ready(val book: ReaderBook) : UiState
    }

    class ReaderBook(
        val session: ReaderSession,
        val publication: Publication,
        val chapters: List<Chapter>,
        val positionRanges: List<ChapterPositionRange>,
        val spineCount: UInt,
    )

    sealed interface LayoutEvent {
        val generation: ULong

        data class FirstPage(override val generation: ULong, val spineIdx: UInt) : LayoutEvent
        data class Chapter(override val generation: ULong, val spineIdx: UInt, val pageCount: UInt) : LayoutEvent
        data class Failed(override val generation: ULong, val spineIdx: UInt) : LayoutEvent
    }

    data class SearchHit(
        val spineIdx: UInt,
        val charOffset: ULong?,
        val snippetPre: String,
        val snippetMatch: String,
        val snippetPost: String,
        /** Unicode-scalar length for the engine's future match-rect request. */
        val matchLength: ULong,
        val position: UInt?,
    )

    data class SearchOutcome(
        val hits: List<SearchHit> = emptyList(),
        val total: Int = 0,
        val unavailable: Boolean = false,
    )

    private data class PendingProgress(val coordinate: Coordinate, val position: UInt, val progression: Double)

    private val stateFlow = MutableStateFlow<UiState>(UiState.Opening)
    val state: StateFlow<UiState> = stateFlow.asStateFlow()

    // Replay bridges callbacks that win the race against AndroidView
    // mounting and re-primes a recreated surface after a config change.
    // Only events of the accepted generation are ever emitted, and the
    // replay cache is purged on relayout, so a collector can never adopt a
    // stale generation from it.
    private val _layoutEvents = MutableSharedFlow<LayoutEvent>(replay = 64, extraBufferCapacity = 64)
    val layoutEvents = _layoutEvents

    @Volatile private var bookshelf: app.inkuna.core.Bookshelf? = null
    private var openJob: Job? = null
    private var sessionId: String? = null
    private val writeLock = Mutex()
    private var writeTail: Job? = null
    private val pendingProgress = MutableStateFlow<PendingProgress?>(null)
    private var lastPersisted: Coordinate? = null
    private var targetCoordinate: Coordinate? = null
    private var currentAnchor: Coordinate? = null
    private var pendingSettle: Pair<UInt, UInt>? = null
    private var readerSession: ReaderSession? = null
    private var initialHrefFailed = false
    private var initialLocation: PageLocation? = null

    // The engine's generation is the sole staleness truth; this pin merely
    // mirrors the latest accepted one, exactly like iOS's layoutGeneration.
    // It lives here — retained across configuration changes — so a rebuilt
    // composition can never re-adopt a stale generation.
    private var layoutGeneration: ULong? = null
    private var generationBeforeLayout: ULong? = null
    private var layoutChangeInFlight = false
    private val pendingLayoutEvents = mutableListOf<LayoutEvent>()
    private var appliedViewport: Viewport? = null
    private var openedAtMs = 0L
    private var firstPageReadyMs: Long? = null
    private var didLogFirstRender = false
    private var didLogChapterComplete = false

    init {
        open()
        viewModelScope.launch(Dispatchers.Default) {
            pendingProgress.filterNotNull().collect { persistPendingProgress() }
        }
    }

    fun open() {
        if (openJob?.isActive == true || stateFlow.value is UiState.Ready) return
        stateFlow.value = UiState.Opening
        openJob = viewModelScope.launch {
            try {
                stateFlow.value = UiState.Ready(doOpen())
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (unsupported: InkunaException.UnsupportedContent) {
                stateFlow.value = UiState.FixedLayoutUnsupported
            } catch (failure: Throwable) {
                Log.w(TAG, "opening $publicationId failed", failure)
                stateFlow.value = UiState.Failed
            }
        }
    }

    private suspend fun doOpen(): ReaderBook = withContext(Dispatchers.Default) {
        openedAtMs = SystemClock.uptimeMillis()
        firstPageReadyMs = null
        didLogFirstRender = false
        didLogChapterComplete = false
        initialHrefFailed = false
        val shelf = LibraryStore.bookshelf(app)
        bookshelf = shelf
        val library = shelf.library()
        val publication = library.publication(publicationId)
        val chapters = library.chapters(publicationId)
        val positionRanges = shelf.progress().chapterPositionRanges(publicationId)
        val openViewport = viewport()
        val session = shelf.openReader(publicationId, openViewport, layoutSettings(AppSettings.get(app).snapshot.value), listener())
        readerSession = session
        appliedViewport = openViewport
        val restoredCoordinate = publication.coordinate
            ?: coordinateForProgression(publication.progression, session)

        targetCoordinate = initialChapterHref?.let { href ->
            runCatching { session.locateHrefParts(href) }
                .onFailure { initialHrefFailed = it is InkunaException.AnchorNotFound || it is InkunaException.NotReady }
                .getOrNull()
        } ?: restoredCoordinate
        if (initialChapterHref != null && targetCoordinate == null) {
            targetCoordinate = restoredCoordinate
        }

        initialLocation = targetCoordinate?.let { runCatching { session.locate(it) }.getOrNull() }
        ReaderBook(
            session = session,
            publication = publication,
            chapters = chapters,
            positionRanges = positionRanges,
            spineCount = session.spineCount(),
        )
    }

    private fun listener() = object : LayoutListener {
        override fun onFirstPageReady(generation: ULong, spineIdx: UInt) {
            postLayoutEvent(LayoutEvent.FirstPage(generation, spineIdx))
        }

        override fun onChapterReady(generation: ULong, spineIdx: UInt, pageCount: UInt) {
            postLayoutEvent(LayoutEvent.Chapter(generation, spineIdx, pageCount))
        }

        override fun onChapterFailed(generation: ULong, spineIdx: UInt) {
            postLayoutEvent(LayoutEvent.Failed(generation, spineIdx))
        }
    }

    private fun postLayoutEvent(event: LayoutEvent) {
        viewModelScope.launch(Dispatchers.Main.immediate) {
            if (layoutChangeInFlight) {
                // Mirrors iOS `receive`: stale-generation events die here;
                // fresh ones wait until the surface has been invalidated.
                if (event.generation != generationBeforeLayout) pendingLayoutEvents += event
                return@launch
            }
            if (acceptGeneration(event.generation)) _layoutEvents.emit(event)
        }
    }

    /** Mirrors iOS `accept`: pins the latest generation, drops the rest. */
    private fun acceptGeneration(generation: ULong): Boolean {
        val pin = layoutGeneration
        if (pin != null) {
            if (generation != pin) return false
        } else if (generation == generationBeforeLayout) {
            return false
        }
        layoutGeneration = generation
        return true
    }

    /** The open-time restore location; consumed exactly once per open. */
    fun takeInitialLocation(): PageLocation? = initialLocation.also { initialLocation = null }

    fun consumeInitialHrefFailure(): Boolean = initialHrefFailed.also { initialHrefFailed = false }

    fun currentCoordinate(): Coordinate? = currentAnchor ?: targetCoordinate

    fun onPageSettled(spineIdx: UInt, pageIdx: UInt) {
        val session = runCatching { session() }.getOrNull() ?: return
        val coordinate = runCatching {
            Coordinate(spineIdx, session.pageCharRange(spineIdx, pageIdx).start)
        }.getOrNull()
        if (coordinate == null) {
            // The engine may have evicted this chapter (LRU beyond the
            // cache capacity while the rest of the book laid out); the
            // miss just re-scheduled it, so finish this settle on the
            // chapter's next layout event instead of dropping the anchor.
            pendingSettle = spineIdx to pageIdx
            return
        }
        pendingSettle = null
        currentAnchor = coordinate
        targetCoordinate = coordinate
        viewModelScope.launch(Dispatchers.Default) {
            val shelf = bookshelf ?: return@launch
            val position = runCatching { ReaderPositions.position(coordinate, publicationId, shelf) }.getOrNull() ?: return@launch
            val progression = runCatching { ReaderPositions.progression(coordinate, publicationId, shelf) }
                .getOrNull() ?: return@launch
            pendingProgress.value = PendingProgress(
                coordinate = coordinate,
                position = position,
                progression = progression,
            )
        }
    }

    private suspend fun persistPendingProgress() {
        enqueueCoreWrite { writePendingProgress() }
    }

    private suspend fun writePendingProgress() {
        val pending = pendingProgress.value ?: return
        if (pending.coordinate == lastPersisted) return
        writeLock.withLock {
            val latest = pendingProgress.value ?: return@withLock
            val shelf = bookshelf ?: return@withLock
            runCatching {
                shelf.progress().updateProgress(publicationId, latest.coordinate, latest.progression, latest.position)
                lastPersisted = latest.coordinate
            }.onFailure { Log.w(TAG, "updateProgress failed", it) }
        }
    }

    private fun retryPendingSettle(spineIdx: UInt) {
        val (spine, page) = pendingSettle ?: return
        if (spine != spineIdx) return
        pendingSettle = null
        onPageSettled(spine, page)
    }

    fun onFirstPageReady(spineIdx: UInt) {
        retryPendingSettle(spineIdx)
        if (spineIdx == targetCoordinate?.spineIdx && firstPageReadyMs == null) {
            firstPageReadyMs = SystemClock.uptimeMillis()
            logPerf("open_to_first_page_ready_ms", openedAtMs)
        }
    }

    fun onChapterReady(spineIdx: UInt) {
        retryPendingSettle(spineIdx)
        if (spineIdx == targetCoordinate?.spineIdx && !didLogChapterComplete) {
            didLogChapterComplete = true
            logPerf("chapter_layout_complete_ms", openedAtMs)
        }
    }

    fun onCurrentPageDrawn() {
        if (didLogFirstRender) return
        didLogFirstRender = true
        firstPageReadyMs?.let { logPerf("first_page_ready_to_first_render_ms", it) }
        ReaderPerf.tapUptimeMs.takeIf { it > 0L }?.let { logPerf("tap_to_first_page_ms", it) }
    }

    /**
     * Relayouts the session; mirrors iOS `relayout`. [onApplied] runs on
     * success — after the generation pin is released, before the buffered
     * new-generation events flush — so the caller invalidates its surface
     * in exactly the window iOS does.
     */
    suspend fun updateAppearance(settings: ReaderLayoutSettings, onApplied: () -> Unit): Boolean =
        withContext(Dispatchers.Main.immediate) {
            val session = session()
            currentAnchor = currentCoordinate()
            // The relayout re-anchor supersedes any settle waiting on the
            // old generation's page numbering.
            pendingSettle = null
            generationBeforeLayout = layoutGeneration
            layoutChangeInFlight = true
            val target = viewport()
            val updated = withContext(Dispatchers.Default) {
                runCatching { session.updateLayout(target, settings) }
                    .onFailure { Log.w(TAG, "reader relayout failed", it) }
                    .isSuccess
            }
            if (updated) {
                layoutGeneration = null
                appliedViewport = target
                _layoutEvents.resetReplayCache()
                onApplied()
            }
            layoutChangeInFlight = false
            val buffered = pendingLayoutEvents.toList()
            pendingLayoutEvents.clear()
            buffered.forEach { if (acceptGeneration(it.generation)) _layoutEvents.emit(it) }
            updated
        }

    /**
     * Whether the window no longer matches the viewport the session laid
     * out for — true after a rotation recreated the activity while this
     * retained session kept the old geometry.
     */
    fun needsViewportRelayout(): Boolean {
        val applied = appliedViewport ?: return false
        return viewport() != applied
    }

    suspend fun search(query: String): SearchOutcome {
        val shelf = bookshelf ?: return SearchOutcome()
        val results = try {
            shelf.search().searchInBook(publicationId, query, SEARCH_LIMIT)
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (failure: Throwable) {
            Log.w(TAG, "searchInBook failed", failure)
            return SearchOutcome(unavailable = true)
        }
        return SearchOutcome(
            hits = results.hits.map { hit -> hit.toSearchHit(results.canonical, shelf) },
            total = results.total.toInt(),
        )
    }

    private suspend fun BookSearchHit.toSearchHit(canonical: Boolean, shelf: app.inkuna.core.Bookshelf): SearchHit {
        val coordinate = takeIf { canonical }?.let { Coordinate(spineIdx, charOffset.toULong()) }
        return SearchHit(
            spineIdx = spineIdx,
            charOffset = coordinate?.charOffset,
            snippetPre = snippetPre,
            snippetMatch = snippetMatch,
            snippetPost = snippetPost,
            matchLength = snippetMatch.codePointCount(0, snippetMatch.length).toULong(),
            position = coordinate?.let { runCatching { ReaderPositions.position(it, publicationId, shelf) }.getOrNull() },
        )
    }

    fun coordinateForBookmark(bookmark: Bookmark, onUnavailable: () -> Unit): Coordinate? =
        bookmark.coordinate ?: readerSession?.let {
            coordinateForProgression(bookmark.progression, it)
        } ?: run {
            onUnavailable()
            null
        }

    private fun coordinateForProgression(progression: Double, session: ReaderSession): Coordinate? {
        val count = session.positionCount()
        if (count == 0u) return null
        val position = minOf(maxOf((progression * count.toDouble()).roundToLong().toUInt(), 1u), count)
        return session.coordinateAtPosition(position)
    }

    fun addBookmark(coordinate: Coordinate, progression: Double, onPlaced: () -> Unit) {
        enqueueCoreWrite {
            val shelf = bookshelf ?: return@enqueueCoreWrite
            runCatching { shelf.library().addBookmark(publicationId, coordinate, progression) }
                .onSuccess { withContext(Dispatchers.Main.immediate) { onPlaced() } }
                .onFailure { Log.w(TAG, "addBookmark failed", it) }
        }
    }

    fun onReaderVisible() {
        enqueueCoreWrite {
            writeLock.withLock {
                if (sessionId == null) sessionId = runCatching { sessionShelf().stats().sessionStart(publicationId) }
                    .onFailure { Log.w(TAG, "sessionStart failed", it) }.getOrNull()
            }
        }
    }

    fun onReaderHidden() {
        enqueueCoreWrite {
            writePendingProgress()
            endSession()
        }
    }

    private suspend fun endSession() {
        writeLock.withLock {
            val id = sessionId ?: return
            sessionId = null
            runCatching { sessionShelf().stats().sessionEnd(id) }
                .onFailure { Log.w(TAG, "sessionEnd failed", it) }
        }
    }

    private fun enqueueCoreWrite(block: suspend () -> Unit): Job {
        val previous = writeTail
        return LibraryStore.writes.launch {
            previous?.join()
            withContext(NonCancellable) { block() }
        }.also { writeTail = it }
    }

    private fun session(): ReaderSession = readerSession
        ?: error("reader session is not open")

    private fun sessionShelf() = bookshelf ?: error("bookshelf is not open")

    private fun viewport(): Viewport {
        val manager = app.getSystemService(WindowManager::class.java)
        val metrics = manager.currentWindowMetrics
        val density = app.resources.displayMetrics.density
        val insets = metrics.windowInsets
        val status = insets.getInsetsIgnoringVisibility(WindowInsets.Type.statusBars()).top
        val cutout = insets.displayCutout?.safeInsetTop ?: 0
        val navigation = insets.getInsetsIgnoringVisibility(WindowInsets.Type.navigationBars()).bottom
        val tablet = app.resources.configuration.smallestScreenWidthDp >= 600
        val top = ReaderMetrics.contentTop(maxOf(status, cutout).toFloat().div(density).dp, tablet).value
        val bottom = ReaderMetrics.contentBottom(navigation.toFloat().div(density).dp, tablet).value
        return Viewport(
            width = metrics.bounds.width().toDouble() / density,
            height = (metrics.bounds.height().toDouble() / density - top - bottom).coerceAtLeast(0.0),
        )
    }

    private fun layoutSettings(snapshot: AppSettings.Snapshot) = snapshot.readerLayoutSettings()

    fun settingsFor(snapshot: AppSettings.Snapshot): ReaderLayoutSettings = snapshot.readerLayoutSettings()

    private fun logPerf(name: String, since: Long) {
        Log.i("InkunaPerf", "$name=${SystemClock.uptimeMillis() - since}")
    }

    override fun onCleared() {
        onReaderHidden()
        openJob?.cancel()
        super.onCleared()
    }

    companion object {
        private const val TAG = "InkunaReader"
        private const val SEARCH_LIMIT = 200u

        fun factory(publicationId: String, initialChapterHref: String? = null) = viewModelFactory {
            initializer { ReaderViewModel(this[AndroidViewModelFactory.APPLICATION_KEY]!!, publicationId, initialChapterHref) }
        }
    }
}

internal fun ReaderSession.locateHrefParts(href: String): Coordinate {
    val index = href.indexOf('#')
    return if (index < 0) locateHref(href, null) else locateHref(href.substring(0, index), href.substring(index + 1))
}
