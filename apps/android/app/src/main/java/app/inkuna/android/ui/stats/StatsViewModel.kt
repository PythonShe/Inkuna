package app.inkuna.android.ui.stats

import android.app.Application
import android.util.Log
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import app.inkuna.android.R
import app.inkuna.android.model.BookRow
import app.inkuna.android.model.LibraryStore
import app.inkuna.core.Shelf
import app.inkuna.core.Sort
import app.inkuna.core.Weekday
import java.time.DayOfWeek
import java.time.ZoneId
import java.time.temporal.WeekFields
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

/**
 * The Stats screen's numbers, every one from the core's recorded reading
 * sessions, bucketed in the device's local time and week convention.
 */
class StatsViewModel(application: Application) : AndroidViewModel(application) {

    /** The core's overview, projected stable for Compose. */
    data class Overview(
        val pagesThisWeek: Int,
        val minutesThisMonth: Int,
        val booksFinishedThisYear: Int,
        /** Day-of-month numbers of the current local month with a session. */
        val readDays: Set<Int>,
    )

    data class UiState(
        /** Null until the first answer lands — the screen shows its title
         *  alone rather than zeros pretending to be data. */
        val overview: Overview? = null,
        val inProgress: List<BookRow> = emptyList(),
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
                val bookshelf = LibraryStore.bookshelf(getApplication())
                // Stats bucketing follows the zone and week convention this
                // device lives in; the zone id (not a fixed offset) is what
                // keeps week/month/year boundaries honest across DST.
                val overview = bookshelf.stats().statsOverview(
                    timezone = ZoneId.systemDefault().id,
                    // The same locale the calendar grid renders with
                    // (LocalConfiguration), not Locale.getDefault() — the
                    // two diverge once per-app language lands, and the
                    // week bucketing must match the drawn first column.
                    weekStart = WeekFields.of(
                        getApplication<Application>().resources.configuration.locales[0]
                    ).firstDayOfWeek.toCoreWeekday(),
                )
                // Reading, not unfinished: "in progress" means actually
                // begun, so a freshly imported book stays off this list.
                val inProgress = bookshelf.library().list(Shelf.READING, Sort.RECENTLY_OPENED)
                val unknownAuthor =
                    getApplication<Application>().getString(R.string.unknown_author)
                val fresh = UiState(
                    overview = Overview(
                        pagesThisWeek = overview.pagesThisWeek.toInt(),
                        minutesThisMonth = overview.minutesThisMonth.toInt(),
                        booksFinishedThisYear = overview.booksFinishedThisYear.toInt(),
                        readDays = overview.readDays.map { it.toInt() }.toSet(),
                    ),
                    inProgress = inProgress.map { BookRow.from(it, unknownAuthor) },
                )
                ensureActive()
                _state.value = fresh
            } catch (cancellation: CancellationException) {
                throw cancellation
            } catch (failure: Throwable) {
                // Keep whatever numbers are already on screen; the library
                // screen owns the recovery path for a library that will
                // not open.
                Log.w(TAG, "The stats overview would not load", failure)
            }
        }
    }

    private companion object {
        const val TAG = "InkunaStats"
    }
}

/** The core's week-start enum for a `java.time` day. */
private fun DayOfWeek.toCoreWeekday(): Weekday = when (this) {
    DayOfWeek.MONDAY -> Weekday.MONDAY
    DayOfWeek.TUESDAY -> Weekday.TUESDAY
    DayOfWeek.WEDNESDAY -> Weekday.WEDNESDAY
    DayOfWeek.THURSDAY -> Weekday.THURSDAY
    DayOfWeek.FRIDAY -> Weekday.FRIDAY
    DayOfWeek.SATURDAY -> Weekday.SATURDAY
    DayOfWeek.SUNDAY -> Weekday.SUNDAY
}
