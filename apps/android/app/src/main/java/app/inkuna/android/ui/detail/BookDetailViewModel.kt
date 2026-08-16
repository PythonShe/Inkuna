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
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import org.json.JSONObject
import org.readium.r2.shared.publication.Locator
import org.readium.r2.shared.util.Url

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
        /** Saved synthetic position, when the stored locator carries one. */
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
                val core = bookshelf.publication(publicationId)
                val chapters = bookshelf.chapters(publicationId)

                // The locator blob is opaque to the core; only Readium
                // parses it. An unreadable blob degrades to the book-wide
                // percentage, never to a crash.
                val locator = core.locator?.let { raw ->
                    runCatching { Locator.fromJSON(JSONObject(raw)) }.getOrNull()
                }
                _state.value = UiState(
                    book = BookRow.from(core, app.getString(R.string.unknown_author)),
                    position = locator?.locations?.position,
                    positionCount = core.positionCount?.toInt(),
                    chapters = chapters.map { chapter ->
                        DetailChapter(
                            numeral = (chapter.idx + 1u).toString(),
                            title = chapter.title,
                            depth = chapter.depth.toInt(),
                            href = chapter.href,
                        )
                    },
                    currentChapterIndex = currentChapterIndex(chapters.map { it.href }, locator),
                )
            } catch (cancellation: kotlinx.coroutines.CancellationException) {
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
     * The chapter the saved position sits in: the first TOC entry whose
     * resource matches the stored locator's — the "several entries in one
     * resource resolve to the first" rule the reader's contents sheet uses.
     * Unlike the sheet, this screen keeps the book closed, so a position in
     * a resource carrying no TOC entry of its own cannot be attributed to
     * the preceding chapter; those books show no highlight rather than a
     * guessed one. Matching is on normalized URLs, not raw href strings, so
     * an entry that merely spells the resource differently still counts
     * (CJK resource names included).
     */
    private fun currentChapterIndex(hrefs: List<String>, locator: Locator?): Int? {
        val here = locator?.href?.removeFragment()?.normalize() ?: return null
        return hrefs
            .indexOfFirst { href -> Url(href)?.removeFragment()?.normalize() == here }
            .takeIf { it >= 0 }
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
