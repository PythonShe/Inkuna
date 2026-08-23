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
        val initialLocation: PageLocation?,
    )

    sealed interface LayoutEvent {
        data class FirstPage(val generation: ULong, val spineIdx: UInt) : LayoutEvent
        data class Chapter(val generation: ULong, val spineIdx: UInt, val pageCount: UInt) : LayoutEvent
        data class Failed(val generation: ULong, val spineIdx: UInt) : LayoutEvent
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

    // Replay bridges callbacks that win the race against AndroidView mounting.
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
    private var readerSession: ReaderSession? = null
    private var initialHrefFailed = false
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
        val session = shelf.openReader(publicationId, viewport(), layoutSettings(AppSettings.get(app).snapshot.value), listener())
        readerSession = session

        targetCoordinate = initialChapterHref?.let { href ->
            runCatching { session.locateHrefParts(href) }
                .onFailure { initialHrefFailed = it is InkunaException.AnchorNotFound || it is InkunaException.NotReady }
                .getOrNull()
        } ?: publication.coordinate ?: Coordinate(0u, 0uL)
        if (initialChapterHref != null && targetCoordinate == null) {
            targetCoordinate = publication.coordinate ?: Coordinate(0u, 0uL)
        }

        val initialLocation = targetCoordinate?.let { runCatching { session.locate(it) }.getOrNull() }
        ReaderBook(
            session = session,
            publication = publication,
            chapters = chapters,
            positionRanges = positionRanges,
            spineCount = session.spineCount(),
            initialLocation = initialLocation,
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
        viewModelScope.launch(Dispatchers.Main.immediate) { _layoutEvents.emit(event) }
    }

    fun resolveInitialLocation(): PageLocation? = targetCoordinate?.let { coordinate ->
        runCatching { session().locate(coordinate) }.getOrNull()
    }

    fun consumeInitialHrefFailure(): Boolean = initialHrefFailed.also { initialHrefFailed = false }

    fun currentCoordinate(): Coordinate? = currentAnchor ?: targetCoordinate

    fun onPageSettled(spineIdx: UInt, pageIdx: UInt) {
        val session = runCatching { session() }.getOrNull() ?: return
        val coordinate = runCatching {
            Coordinate(spineIdx, session.pageCharRange(spineIdx, pageIdx).start)
        }.getOrNull() ?: return
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

    fun onFirstPageReady(spineIdx: UInt) {
        if (spineIdx == targetCoordinate?.spineIdx && firstPageReadyMs == null) {
            firstPageReadyMs = SystemClock.uptimeMillis()
            logPerf("open_to_first_page_ready_ms", openedAtMs)
        }
    }

    fun onChapterReady(spineIdx: UInt) {
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

    suspend fun updateAppearance(settings: ReaderLayoutSettings): Boolean = withContext(Dispatchers.Default) {
        val session = session()
        currentAnchor = currentCoordinate()
        return@withContext runCatching { session.updateLayout(viewport(), settings) }
            .onFailure { Log.w(TAG, "reader relayout failed", it) }
            .isSuccess
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

    /**
     * The generated Android bindings do not expose `coordinateAtPosition`, so
     * a legacy coordinate-less bookmark cannot be resolved safely. The caller
     * supplies the existing link-not-followed toast rather than silently doing
     * nothing.
     */
    fun coordinateForBookmark(bookmark: Bookmark, onUnavailable: () -> Unit): Coordinate? =
        bookmark.coordinate ?: run {
            onUnavailable()
            null
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
