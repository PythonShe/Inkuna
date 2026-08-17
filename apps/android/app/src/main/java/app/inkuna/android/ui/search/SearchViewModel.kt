package app.inkuna.android.ui.search

import android.app.Application
import android.util.Log
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import app.inkuna.android.R
import app.inkuna.android.model.BookRow
import app.inkuna.android.model.LibraryStore
import app.inkuna.android.ui.reader.isSearchable
import app.inkuna.core.Shelf
import app.inkuna.core.Sort
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

/**
 * The Search tab's core-backed state: library search through the core's
 * case-folded, CJK-safe metadata matching, and the "Recently added" shelf
 * of the freshest imports while the query is empty.
 */
class SearchViewModel(application: Application) : AndroidViewModel(application) {

    /**
     * One full-text hit: the book, and the core's three-piece excerpt so a
     * row can light the matched run. `match` can be empty — the core then
     * hands back the resource's opening text as `post`.
     */
    data class FullTextRow(
        val book: BookRow,
        val excerptPre: String,
        val excerptMatch: String,
        val excerptPost: String,
    )

    data class UiState(
        val query: String = "",
        /** Search hits for the trimmed query; meaningless while empty. */
        val results: List<BookRow> = emptyList(),
        /** Books whose *text* matches, ranked; empty hides the section. */
        val fullText: List<FullTextRow> = emptyList(),
        /** The discover shelf; empty hides it (an empty library has
         *  nothing to discover). */
        val discover: List<BookRow> = emptyList(),
        /** True once a query's answer has landed — "no results yet" and
         *  "no results" must render differently. */
        val searched: Boolean = false,
        /** The library would not open; worth saying plainly. */
        val failed: Boolean = false,
    )

    private val _state = MutableStateFlow(UiState())
    val state: StateFlow<UiState> = _state.asStateFlow()

    /** The in-flight fetch, so a fast typist's keystrokes cancel their
     *  predecessors instead of racing each other onto the list. */
    private var reload: Job? = null

    init {
        reload()
    }

    fun setQuery(query: String) {
        _state.value = _state.value.copy(query = query, searched = false)
        reload()
    }

    fun reload() {
        reload?.cancel()
        val trimmed = _state.value.query.trim()
        reload = viewModelScope.launch {
            try {
                // Typing coalesces, exactly like the library screen: each
                // keystroke cancels its predecessor while it waits.
                if (trimmed.isNotEmpty()) delay(SEARCH_DEBOUNCE_MS)
                val bookshelf = LibraryStore.bookshelf(getApplication())
                val unknownAuthor =
                    getApplication<Application>().getString(R.string.unknown_author)
                if (trimmed.isEmpty()) {
                    // The freshest imports, a shelf's worth at most.
                    val discover = bookshelf.list(Shelf.ALL, Sort.RECENTLY_ADDED)
                        .take(DISCOVER_CAPACITY)
                        .map { BookRow.from(it, unknownAuthor) }
                    ensureActive()
                    _state.value = _state.value.copy(
                        results = emptyList(),
                        fullText = emptyList(),
                        discover = discover,
                        searched = false,
                        failed = false,
                    )
                } else {
                    val results = bookshelf.searchLibrary(trimmed)
                        .map { BookRow.from(it, unknownAuthor) }
                    ensureActive()
                    // Titles and authors answer first; the text of the books
                    // follows, and only for a query worth reading pages for
                    // (the reader's own rule — one CJK character counts).
                    val fullText = if (isSearchable(trimmed)) {
                        try {
                            bookshelf.searchAllBooks(trimmed, FULL_TEXT_CAPACITY)
                                .map { hit ->
                                    FullTextRow(
                                        book = BookRow.from(hit.publication, unknownAuthor),
                                        excerptPre = hit.excerptPre,
                                        excerptMatch = hit.excerptMatch,
                                        excerptPost = hit.excerptPost,
                                    )
                                }
                        } catch (cancellation: CancellationException) {
                            throw cancellation
                        } catch (failure: Throwable) {
                            // The index is derived data: a search that is
                            // unavailable quietly leaves the section out
                            // rather than failing the whole screen.
                            Log.w(TAG, "Full-text search is unavailable", failure)
                            emptyList()
                        }
                    } else {
                        emptyList()
                    }
                    ensureActive()
                    _state.value = _state.value.copy(
                        results = results,
                        fullText = fullText,
                        searched = true,
                        failed = false,
                    )
                }
            } catch (cancellation: CancellationException) {
                throw cancellation
            } catch (failure: Throwable) {
                Log.e(TAG, "The library would not open", failure)
                _state.value = _state.value.copy(
                    results = emptyList(),
                    fullText = emptyList(),
                    discover = emptyList(),
                    searched = false,
                    failed = true,
                )
            }
        }
    }

    private companion object {
        const val TAG = "InkunaSearch"

        /** The library screen's debounce, for the same reason. */
        const val SEARCH_DEBOUNCE_MS = 200L

        /** A shelf's worth of discoveries. */
        const val DISCOVER_CAPACITY = 8

        /** One screenful of books whose text matches — never the library. */
        const val FULL_TEXT_CAPACITY = 20u
    }
}
