package app.inkuna.android.ui.reader

import android.os.SystemClock

/**
 * Whether a text-selection action mode is live anywhere in the activity.
 *
 * The WebView starts its selection ActionMode through the activity
 * (`startActionModeForChild` bubbles up), so `onActionModeStarted`/
 * `onActionModeFinished` overrides in `MainActivity` see every selection —
 * without owning the selection menu, which supplying Readium's
 * `selectionActionModeCallback` would force. The reader's pager consults
 * this before claiming a horizontal drag: while a selection is up, drags
 * belong to its handles, never to page turns.
 *
 * A counter rather than a boolean: overlapping modes (B starts before A
 * finishes) must not read as "no selection", and a missed `finished`
 * must not kill page turns for the life of the process — the reader
 * calls [reset] on teardown as the backstop. Main-thread callers only.
 */
object SelectionModeTracker {
    @Volatile
    private var liveModes = 0

    @Volatile
    private var lastStartUptime = 0L

    val active: Boolean get() = liveModes > 0

    fun started() {
        liveModes++
        lastStartUptime = SystemClock.uptimeMillis()
    }

    fun finished() {
        if (liveModes > 0) liveModes--
    }

    /** Whether a selection mode started at or after [uptime] — catches a
     *  handle grabbed in the beat before the mode's start callback ran. */
    fun startedSince(uptime: Long): Boolean = lastStartUptime >= uptime

    fun reset() {
        liveModes = 0
    }
}
