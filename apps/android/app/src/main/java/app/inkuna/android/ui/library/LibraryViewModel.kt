package app.inkuna.android.ui.library

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import app.inkuna.android.model.LibraryStore
import app.inkuna.core.Publication
import app.inkuna.core.Shelf
import app.inkuna.core.Sort
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

/** Which segment of the library is on screen. */
enum class LibrarySegment { Reading, Finished, Wishlist }

/**
 * One row, projected off the core's [Publication].
 *
 * Deliberately not the generated type: UniFFI records expose `var` fields,
 * which Compose treats as unstable and recomposes on every read.
 */
data class LibraryRow(
    val id: String,
    val title: String,
    val author: String,
    val progress: Int?,
    val seed: Int,
)

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
    val rows: List<LibraryRow> = emptyList(),
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
                val bookshelf = LibraryStore.bookshelf(getApplication())
                val publications = if (trimmed.isEmpty()) {
                    bookshelf.list(shelf, Sort.RECENTLY_OPENED)
                } else {
                    // Metadata search is the core's. The shelf still filters
                    // the result, so a search inside Finished stays there.
                    val shelved = bookshelf.list(shelf, Sort.RECENTLY_OPENED).mapTo(HashSet()) { it.id }
                    bookshelf.searchLibrary(trimmed).filter { it.id in shelved }
                }

                // Distinguish "this shelf is empty" from "there are no books
                // at all" — only the latter earns the invitation to import,
                // and it costs a second query only when there is nothing.
                val emptiness = when {
                    publications.isNotEmpty() -> LibraryEmptiness.Shelf(LibraryEmptiness.Shelf.Kind.Reading)
                    trimmed.isNotEmpty() -> LibraryEmptiness.Shelf(LibraryEmptiness.Shelf.Kind.NoMatches)
                    bookshelf.list(Shelf.ALL, Sort.RECENTLY_ADDED).isEmpty() -> LibraryEmptiness.WholeLibrary
                    current.segment == LibrarySegment.Finished ->
                        LibraryEmptiness.Shelf(LibraryEmptiness.Shelf.Kind.Finished)
                    else -> LibraryEmptiness.Shelf(LibraryEmptiness.Shelf.Kind.Reading)
                }

                _state.value = _state.value.copy(rows = publications.map(::row), emptiness = emptiness)
            } catch (cancellation: kotlinx.coroutines.CancellationException) {
                throw cancellation
            } catch (failure: Throwable) {
                // A library that will not open is worth saying plainly
                // rather than showing as an empty shelf.
                _state.value = _state.value.copy(
                    rows = emptyList(),
                    emptiness = LibraryEmptiness.Shelf(LibraryEmptiness.Shelf.Kind.Unopenable),
                )
            }
        }
    }

    private fun row(publication: Publication) = LibraryRow(
        id = publication.id,
        title = publication.title,
        author = publication.authors.joinToString(", ").ifEmpty { null } ?: UNKNOWN_AUTHOR,
        progress = publication.progression.takeIf { it > 0.0 }?.let { (it * 100).toInt() },
        seed = coverSeed(publication.id),
    )

    private companion object {
        // TODO(l10n): move to strings.xml with the rest of the l10n pass.
        const val UNKNOWN_AUTHOR = "Unknown author"

        /**
         * A stable per-book seed for the generated cover.
         *
         * Deliberately not [String.hashCode]: FNV-1a keeps the colour of a
         * book identical across processes and platforms.
         *
         * TODO(core): the core already extracts real cover art into
         * `Publication.coverPath`; render it once `BookCover` accepts an
         * image and falls back to the generated cover.
         */
        fun coverSeed(id: String): Int {
            var hash = -0x340d631b7bdddcdbL // FNV-1a 64-bit offset basis
            for (byte in id.toByteArray()) {
                hash = hash xor (byte.toLong() and 0xff)
                hash *= 0x100000001b3L
            }
            return (hash ushr 1).toInt() and Int.MAX_VALUE
        }
    }
}
