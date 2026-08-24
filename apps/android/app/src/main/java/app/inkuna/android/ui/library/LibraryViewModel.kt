package app.inkuna.android.ui.library

import android.app.Application
import android.util.Log
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import app.inkuna.android.R
import app.inkuna.android.model.BookRow
import app.inkuna.android.model.LibraryStore
import app.inkuna.core.Publication
import app.inkuna.core.Shelf
import app.inkuna.core.Sort
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

/** Which segment of the library is on screen. */
enum class LibrarySegment { Reading, Finished, Wishlist }

/** What to show when there are no rows. */
sealed interface LibraryEmptiness {
    /** The library holds books, but this shelf or search has none. */
    data class Shelf(val kind: Kind) : LibraryEmptiness {
        enum class Kind { Reading, Finished, Wishlist, NoMatches, Unopenable }
    }

    /** The library holds no books at all — the one case that invites import. */
    data object WholeLibrary : LibraryEmptiness
}

data class LibraryUiState(
    val rows: List<BookRow> = emptyList(),
    val emptiness: LibraryEmptiness = LibraryEmptiness.Shelf(LibraryEmptiness.Shelf.Kind.Reading),
    val segment: LibrarySegment = LibrarySegment.Reading,
    val query: String = "",
)

/**
 * The library screen's core-backed state.
 *
 * Shelf membership is the core's decision, never this layer's: each segment
 * asks for its own [Shelf] rather than fetching everything and re-deriving
 * "reading" or "finished" in Kotlin, which would duplicate core logic and
 * let the two shells drift apart.
 */
class LibraryViewModel(application: Application) : AndroidViewModel(application) {

    private val _state = MutableStateFlow(LibraryUiState())
    val state: StateFlow<LibraryUiState> = _state.asStateFlow()

    /** The in-flight fetch, so a fast typist's keystrokes cancel their
     *  predecessors instead of racing each other onto the list. */
    private var reload: Job? = null

    init {
        reload()
    }

    fun setQuery(query: String) {
        _state.value = _state.value.copy(query = query)
        reload()
    }

    fun setSegment(segment: LibrarySegment) {
        _state.value = _state.value.copy(segment = segment)
        reload()
    }

    fun reload() {
        reload?.cancel()

        val current = _state.value
        // TODO(core): no Wishlist shelf exists yet — file-less publications
        // are deferred to their own spec, so the segment stays empty.
        if (current.segment == LibrarySegment.Wishlist) {
            _state.value = current.copy(
                rows = emptyList(),
                emptiness = LibraryEmptiness.Shelf(LibraryEmptiness.Shelf.Kind.Wishlist),
            )
            return
        }

        // Unfinished, not Reading: a book must be listed the moment it is
        // imported, and Reading deliberately means "opened at least once".
        val shelf = if (current.segment == LibrarySegment.Finished) Shelf.FINISHED else Shelf.UNFINISHED
        val trimmed = current.query.trim()

        reload = viewModelScope.launch {
            try {
                // Typing coalesces: each keystroke cancels its predecessor
                // while it waits, so a four-letter query reaches the core
                // once instead of four times. Every reload with a query
                // present waits out the debounce — only an empty field
                // (segment switches and the initial load included) repaints
                // at once.
                if (trimmed.isNotEmpty()) delay(SEARCH_DEBOUNCE_MS)
                val bookshelf = LibraryStore.bookshelf(getApplication())
                val publications = if (trimmed.isEmpty()) {
                    bookshelf.library().list(shelf, Sort.RECENTLY_OPENED)
                } else {
                    // Metadata search is the core's. The shelf still filters
                    // the result, so a search inside Finished stays there.
                    val shelved = bookshelf.library().list(shelf, Sort.RECENTLY_OPENED).mapTo(HashSet()) { it.id }
                    bookshelf.library().searchLibrary(trimmed).filter { it.id in shelved }
                }

                // Distinguish "this shelf is empty" from "there are no books
                // at all" — only the latter earns the invitation to import,
                // and it costs a second query only when there is nothing.
                val emptiness = when {
                    publications.isNotEmpty() -> LibraryEmptiness.Shelf(LibraryEmptiness.Shelf.Kind.Reading)
                    trimmed.isNotEmpty() -> LibraryEmptiness.Shelf(LibraryEmptiness.Shelf.Kind.NoMatches)
                    bookshelf.library().list(Shelf.ALL, Sort.RECENTLY_ADDED).isEmpty() -> LibraryEmptiness.WholeLibrary
                    current.segment == LibrarySegment.Finished ->
                        LibraryEmptiness.Shelf(LibraryEmptiness.Shelf.Kind.Finished)
                    else -> LibraryEmptiness.Shelf(LibraryEmptiness.Shelf.Kind.Reading)
                }

                // A reload superseded mid-flight must not repaint over its
                // successor's rows once the successor has landed.
                ensureActive()
                _state.value = _state.value.copy(rows = publications.map(::row), emptiness = emptiness)
            } catch (cancellation: kotlinx.coroutines.CancellationException) {
                throw cancellation
            } catch (failure: Throwable) {
                // A library that will not open is worth saying plainly
                // rather than showing as an empty shelf — and worth logging,
                // so the cause is not lost behind the message.
                Log.e(TAG, "The library would not open", failure)
                _state.value = _state.value.copy(
                    rows = emptyList(),
                    emptiness = LibraryEmptiness.Shelf(LibraryEmptiness.Shelf.Kind.Unopenable),
                )
            }
        }
    }

    private fun row(publication: Publication) = BookRow.from(
        publication,
        unknownAuthor = getApplication<Application>().getString(R.string.unknown_author),
    )

    private companion object {
        const val TAG = "InkunaLibrary"

        /** Long enough to swallow a fast typist's keystrokes, short enough
         *  that a reader who stops typing does not notice waiting. */
        const val SEARCH_DEBOUNCE_MS = 200L
    }
}
