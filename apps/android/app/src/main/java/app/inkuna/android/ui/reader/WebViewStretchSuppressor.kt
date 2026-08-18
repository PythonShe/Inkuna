package app.inkuna.android.ui.reader

import android.view.View
import android.view.ViewGroup
import android.view.ViewTreeObserver
import android.webkit.WebView

/**
 * Disables the overscroll stretch on every WebView under the navigator.
 *
 * Readium's paginated EPUB puts each resource in a WebView whose columns
 * clamp at the resource's edges; the toolkit never touches [View.setOverScrollMode],
 * so a drag past the last page hits Android's default stretch edge effect —
 * the page visually rubber-bands instead of turning. Readium's own iOS
 * navigator disables the equivalent (`scrollView.bounces = false`); this
 * class brings Android to parity.
 *
 * There is no toolkit hook for it, so the suppressor re-walks the navigator's
 * view tree on every layout pass and flips any WebView it finds (the pager
 * creates and recycles resource WebViews as the reader moves through the
 * book). The walk touches a handful of views, uses only public APIs, and
 * degrades to a no-op if a future toolkit stops nesting WebViews there.
 */
class WebViewStretchSuppressor(
    private val root: View,
) : ViewTreeObserver.OnGlobalLayoutListener {

    fun attach() {
        root.viewTreeObserver.addOnGlobalLayoutListener(this)
        onGlobalLayout()
    }

    fun detach() {
        // Always resolve the observer fresh: the one attach() saw dies if
        // the view was attached to a window afterwards.
        root.viewTreeObserver.takeIf { it.isAlive }?.removeOnGlobalLayoutListener(this)
    }

    override fun onGlobalLayout() = suppress(root)

    private fun suppress(view: View) {
        if (view is WebView && view.overScrollMode != View.OVER_SCROLL_NEVER) {
            view.overScrollMode = View.OVER_SCROLL_NEVER
        }
        if (view is ViewGroup) {
            for (i in 0 until view.childCount) suppress(view.getChildAt(i))
        }
    }
}
