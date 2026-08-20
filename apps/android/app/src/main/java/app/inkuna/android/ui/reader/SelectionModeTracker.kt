package app.inkuna.android.ui.reader

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
 */
object SelectionModeTracker {
    @Volatile
    var active: Boolean = false
}
