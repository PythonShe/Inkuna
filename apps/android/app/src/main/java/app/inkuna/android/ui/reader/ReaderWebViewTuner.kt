package app.inkuna.android.ui.reader

import android.os.Build
import android.view.View
import android.view.ViewGroup
import android.webkit.WebView
import androidx.viewpager.widget.ViewPager

/**
 * Tunes every resource WebView (and the resource pager) the toolkit
 * creates under the navigator:
 *
 * - **Overscroll stretch off.** Columns clamp at resource edges and the
 *   toolkit never touches [View.setOverScrollMode], so a drag past the
 *   last page paints Android's stretch edge effect instead of turning.
 *   Readium's own iOS navigator disables the equivalent
 *   (`scrollView.bounces = false`); this brings Android to parity.
 * - **High frame-rate vote (API 35+).** On adaptive-refresh displays a
 *   view redraws in the 60 Hz "normal" category unless it votes higher,
 *   and votes do not propagate from parent to child — every view that
 *   redraws needs its own. The WebViews carry the page scroll and the
 *   pager carries the boundary slide, so both vote high for as long as
 *   they live; the votes die with the views when the reader closes.
 *
 * Discovery rides a hierarchy listener installed down the tree (the
 * pager creates and recycles resource WebViews as the reader moves
 * through the book), so the work happens when views appear rather than
 * on every layout pass of the window. The toolkit's containers set no
 * hierarchy listeners of their own, the walk stops at each WebView, and
 * everything degrades to a no-op if a future toolkit changes shape.
 */
class ReaderWebViewTuner(private val root: View) {

    private val listener = object : ViewGroup.OnHierarchyChangeListener {
        override fun onChildViewAdded(parent: View, child: View) = install(child)
        override fun onChildViewRemoved(parent: View, child: View) = Unit
    }

    fun attach() = install(root)

    fun detach() = uninstall(root)

    private fun install(view: View) {
        if (view is WebView) {
            if (view.overScrollMode != View.OVER_SCROLL_NEVER) {
                view.overScrollMode = View.OVER_SCROLL_NEVER
            }
            voteHigh(view)
            return
        }
        if (view is ViewPager) voteHigh(view)
        if (view is ViewGroup) {
            view.setOnHierarchyChangeListener(listener)
            for (i in 0 until view.childCount) install(view.getChildAt(i))
        }
    }

    private fun uninstall(view: View) {
        if (view is WebView) return
        if (view is ViewGroup) {
            view.setOnHierarchyChangeListener(null)
            for (i in 0 until view.childCount) uninstall(view.getChildAt(i))
        }
    }

    private fun voteHigh(view: View) {
        if (Build.VERSION.SDK_INT >= 35) {
            view.requestedFrameRate = View.REQUESTED_FRAME_RATE_CATEGORY_HIGH
        }
    }
}
