package app.inkuna.android.ui.detail

import android.app.Application
import android.util.Log
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.ViewModelProvider.AndroidViewModelFactory
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import app.inkuna.android.R
import app.inkuna.android.model.BookRow
import app.inkuna.android.model.LibraryStore
import app.inkuna.android.ui.reader.ReaderPositions
import app.inkuna.core.Chapter
import app.inkuna.core.ChapterPositionRange
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

/**
 * The detail screen's core-backed state: the publication, its flattened
 * TOC, and where the saved position sits in it. Re-fetched on every
 * appearance — progress moves while the reader is open, and the screen
 * behind it is stale by the time the reader is popped.
 */
class BookDetailViewModel(
    private val app: Application,
    private val publicationId: String,
) : AndroidViewModel(app) {

    /** One TOC row, projected stable for Compose off the core's record. */
    data class DetailChapter(
        val numeral: String,
        val title: String,
        val depth: Int,
        /** The core's package-root-relative jump target. */
        val href: String,
    )

    data class UiState(
        val book: BookRow? = null,
        /** Saved synthetic position, when the book carries a coordinate
         *  the core can resolve to one. */
        val position: Int? = null,
        val positionCount: Int? = null,
        val chapters: List<DetailChapter> = emptyList(),
        val currentChapterIndex: Int? = null,
        /** The book could not be fetched at all (nothing to stand on). */
        val failed: Boolean = false,
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
                val bookshelf = LibraryStore.bookshelf(app)
                val core = bookshelf.library().publication(publicationId)
                val chapters = bookshelf.library().chapters(publicationId)

                // The position line and the chapter highlight both hang on
                // the stored coordinate; without one there is nothing to
                // ask the core about, and both degrade rather than guess.
                // A book the core cannot place is degraded the same way
                // rather than failing the screen — the cover, the blurb and
                // the contents are all still worth showing.
                val coordinate = core.coordinate
                var position: UInt? = null
                var ranges: List<ChapterPositionRange> = emptyList()
                if (coordinate != null) {
                    try {
                        position = ReaderPositions.position(coordinate, publicationId, bookshelf)
                        ranges = bookshelf.progress().chapterPositionRanges(publicationId)
                    } catch (cancellation: CancellationException) {
                        throw cancellation
                    } catch (failure: Throwable) {
                        Log.w(TAG, "The saved position for $publicationId would not resolve", failure)
                        position = null
                        ranges = emptyList()
                    }
                }
                _state.value = UiState(
                    book = BookRow.from(core, app.getString(R.string.unknown_author)),
                    position = position?.toInt(),
                    positionCount = core.positionCount?.toInt(),
                    chapters = chapters.map { chapter ->
                        DetailChapter(
                            numeral = (chapter.idx + 1u).toString(),
                            title = chapter.title,
                            depth = chapter.depth.toInt(),
                            href = chapter.href,
                        )
                    },
                    currentChapterIndex = currentChapterIndex(chapters, ranges, position),
                )
            } catch (cancellation: CancellationException) {
                throw cancellation
            } catch (failure: Throwable) {
                Log.w(TAG, "Detail for $publicationId would not load", failure)
                // A failed refresh keeps what is already on screen; only a
                // screen with nothing to stand on reports failure.
                if (_state.value.book == null) {
                    _state.value = _state.value.copy(failed = true)
                }
            }
        }
    }

    /**
     * The chapter the saved position sits in, attributed by the core's own
     * chapter spans rather than by matching hrefs here — the same rule the
     * reader's contents sheet highlights by. A position no span claims
     * leaves the list unhighlighted rather than guessed.
     */
    private fun currentChapterIndex(
        chapters: List<Chapter>,
        ranges: List<ChapterPositionRange>,
        position: UInt?,
    ): Int? {
        if (position == null) return null
        val range = ReaderPositions.chapterRange(ranges, position) ?: return null
        return chapters.indexOfFirst { it.idx == range.chapterIdx }.takeIf { it >= 0 }
    }

    companion object {
        private const val TAG = "InkunaDetail"

        fun factory(publicationId: String) = viewModelFactory {
            initializer {
                val application = this[AndroidViewModelFactory.APPLICATION_KEY]!!
                BookDetailViewModel(application, publicationId)
            }
        }
    }
}
