package app.inkuna.android.ui.tonight

import android.app.Application
import android.util.Log
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import app.inkuna.android.R
import app.inkuna.android.model.BookRow
import app.inkuna.android.model.LibraryStore
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
                val publications = LibraryStore.bookshelf(getApplication())
                    .list(Shelf.UNFINISHED, Sort.RECENTLY_OPENED)
                val unknownAuthor =
                    getApplication<Application>().getString(R.string.unknown_author)
                val rows = publications.map { BookRow.from(it, unknownAuthor) }
                ensureActive()
                // A successful-but-empty answer clears the hero; only a
                // failed load keeps whatever the card already shows — the
                // library screen owns the recovery path for a library that
                // will not open.
                _state.value = UiState(
                    continueReading = rows.firstOrNull(),
                    nightstand = rows.drop(1).take(NIGHTSTAND_CAPACITY),
                )
            } catch (cancellation: CancellationException) {
                throw cancellation
            } catch (failure: Throwable) {
                Log.w(TAG, "The tonight shelf would not load", failure)
            }
        }
    }

    private companion object {
        const val TAG = "InkunaTonight"

        /** A shelf's worth: most recently touched first, never the whole
         *  library sideways. */
        const val NIGHTSTAND_CAPACITY = 8
    }
}
