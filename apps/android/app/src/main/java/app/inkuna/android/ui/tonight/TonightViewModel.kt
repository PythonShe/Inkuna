package app.inkuna.android.ui.tonight

import android.app.Application
import android.util.Log
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import app.inkuna.android.R
import app.inkuna.android.model.BookRow
import app.inkuna.android.model.LibraryStore
import app.inkuna.android.ui.reader.ReaderPositions
import app.inkuna.core.Bookshelf
import app.inkuna.core.Publication
import app.inkuna.core.Shelf
import app.inkuna.core.Sort
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

/**
 * The Tonight screen's core-backed state: the hero to continue and the
 * nightstand shelf behind it, both cut from one unfinished-shelf query.
 */
class TonightViewModel(application: Application) : AndroidViewModel(application) {

    data class UiState(
        /** The most recently opened unfinished book; null renders the
         *  design's placeholder card — inert scenery, never a destination. */
        val continueReading: BookRow? = null,
        /** The rest of the unfinished pile; empty hides the section. */
        val nightstand: List<BookRow> = emptyList(),
        /** Synthetic positions left in the hero's current chapter, when
         *  the core knows the chapter's range and the reader is inside it;
         *  null falls the caption back to the book-wide percentage. */
        val pagesLeftInChapter: Int? = null,
    )

    private val _state = MutableStateFlow(UiState())
    val state: StateFlow<UiState> = _state.asStateFlow()

    private var reload: Job? = null

    init {
        reload()
    }

    fun reload() {
        reload?.cancel()
        reload = viewModelScope.launch {
            try {
                // Unfinished, not all: a book just finished must not be the
                // "keep reading" hero merely for being touched last.
                val bookshelf = LibraryStore.bookshelf(getApplication())
                val publications = bookshelf.library().list(Shelf.UNFINISHED, Sort.RECENTLY_OPENED)
                val unknownAuthor =
                    getApplication<Application>().getString(R.string.unknown_author)
                val rows = publications.map { BookRow.from(it, unknownAuthor) }
                val pagesLeft = publications.firstOrNull()?.let { pagesLeftInChapter(bookshelf, it) }
                ensureActive()
                // A successful-but-empty answer clears the hero; only a
                // failed load keeps whatever the card already shows — the
                // library screen owns the recovery path for a library that
                // will not open.
                _state.value = UiState(
                    continueReading = rows.firstOrNull(),
                    nightstand = rows.drop(1).take(NIGHTSTAND_CAPACITY),
                    pagesLeftInChapter = pagesLeft,
                )
            } catch (cancellation: CancellationException) {
                throw cancellation
            } catch (failure: Throwable) {
                Log.w(TAG, "The tonight shelf would not load", failure)
            }
        }
    }

    /**
     * How many synthetic positions the hero has left in the chapter it is
     * stopped in, for the caption only.
     *
     * Both halves are core answers: the position the stored coordinate
     * lands on, and the chapter spans around it. A book with no coordinate
     * yet — never opened, or a legacy row the rebaseline has not reached —
     * has no position to caption and degrades to the percentage line, as
     * does any book whose spans the core cannot produce. Zero or less is
     * not a caption worth showing.
     */
    private suspend fun pagesLeftInChapter(bookshelf: Bookshelf, publication: Publication): Int? {
        val coordinate = publication.coordinate ?: return null
        return try {
            val position = ReaderPositions.position(coordinate, publication.id, bookshelf)
            val ranges = bookshelf.progress().chapterPositionRanges(publication.id)
            val containing = ReaderPositions.chapterRange(ranges, position) ?: return null
            val left = containing.endPosition.toLong() - position.toLong()
            if (left > 0) left.toInt() else null
        } catch (cancellation: CancellationException) {
            throw cancellation
        } catch (failure: Throwable) {
            // The shelf itself loaded; only the caption is poorer for it.
            Log.w(TAG, "The hero's chapter position would not resolve", failure)
            null
        }
    }

    private companion object {
        const val TAG = "InkunaTonight"

        /** A shelf's worth: most recently touched first, never the whole
         *  library sideways. */
        const val NIGHTSTAND_CAPACITY = 8
    }
}
