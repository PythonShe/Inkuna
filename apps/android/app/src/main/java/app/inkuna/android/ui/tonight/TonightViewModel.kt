package app.inkuna.android.ui.tonight

import android.app.Application
import android.util.Log
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import app.inkuna.android.R
import app.inkuna.android.model.BookRow
import app.inkuna.android.model.LibraryStore
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
import org.json.JSONObject
import org.readium.r2.shared.publication.Locator

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
                val publications = bookshelf.list(Shelf.UNFINISHED, Sort.RECENTLY_OPENED)
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
     * How many Readium synthetic positions the hero has left in the chapter
     * it is stopped in.
     *
     * The chapter ranges are reported by the reader the last time the book
     * was open, so they are absent until then — and the stored locator is
     * opaque to the core, so only Readium parses it. Either missing makes
     * this null and the card falls back to the honest percentage. Several
     * ranges can contain one position (a chapter with sub-entries in the
     * same resource); the innermost — the greatest start — is the chapter
     * the reader is actually in.
     */
    private suspend fun pagesLeftInChapter(bookshelf: Bookshelf, publication: Publication): Int? {
        val position = publication.locator
            ?.let { raw -> runCatching { Locator.fromJSON(JSONObject(raw)) }.getOrNull() }
            ?.locations?.position
            ?: return null
        val ranges = try {
            bookshelf.chapterPositionRanges(publication.id)
        } catch (cancellation: CancellationException) {
            throw cancellation
        } catch (failure: Throwable) {
            Log.w(TAG, "Chapter position ranges would not load", failure)
            return null
        }
        val here = ranges
            .filter { position.toUInt() in it.startPosition..it.endPosition }
            .maxByOrNull { it.startPosition }
            ?: return null
        return (here.endPosition.toLong() - position).toInt().takeIf { it > 0 }
    }

    private companion object {
        const val TAG = "InkunaTonight"

        /** A shelf's worth: most recently touched first, never the whole
         *  library sideways. */
        const val NIGHTSTAND_CAPACITY = 8
    }
}
