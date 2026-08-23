package app.inkuna.android.ui.reader.engine

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Rect
import android.graphics.RectF
import android.view.ActionMode
import android.view.HapticFeedbackConstants
import android.view.MotionEvent
import android.view.View
import app.inkuna.android.R
import app.inkuna.android.ui.reader.SelectionModeTracker
import app.inkuna.core.CharRange
import app.inkuna.core.InkunaException
import app.inkuna.core.ReaderSession
import app.inkuna.core.SelectionRect
import app.inkuna.core.WritingMode
import kotlin.math.roundToInt

/** Native text selection over one published engine page. */
class ReaderSelectionController(
    private val session: ReaderSession,
    private val canvas: EnginePageCanvas,
    private val surface: EnginePagerSurface,
) {
    private data class ActiveSelection(
        val spineIdx: UInt,
        val pageIdx: UInt,
        val pageRange: CharRange,
        var range: CharRange,
    )

    private enum class Handle { START, END }

    private data class HandleDrag(val anchor: ULong)

    private var active: ActiveSelection? = null
        set(value) {
            field = value
            surface.selectionActive = value != null
            if (value == null) SelectionModeTracker.finished()
            else SelectionModeTracker.started()
        }
    private var dragging: HandleDrag? = null
    private var actionMode: ActionMode? = null
    private var searchHighlightDismissal: Runnable? = null
    private var searchHighlightToken = 0
    private val overlay = SelectionOverlay(canvas.context) { handle, event, x, y ->
        when (event) {
            MotionEvent.ACTION_DOWN -> beginHandleDrag(handle)
            MotionEvent.ACTION_MOVE -> updateHandle(x, y)
            MotionEvent.ACTION_UP -> {
                updateHandle(x, y)
                dragging = null
                presentActionMode()
            }
            MotionEvent.ACTION_CANCEL -> dragging = null
        }
    }

    init {
        canvas.installSelectionOverlay(overlay)
        canvas.selectionController = this
    }

    val isActive: Boolean get() = active != null

    fun beginAt(spineIdx: UInt, pageIdx: UInt, x: Float, y: Float) {
        if (isActive) return
        cancelSearchHighlight()
        overlay.clear()
        try {
            val pageRange = session.pageCharRange(spineIdx, pageIdx)
            val hit = session.hitTest(spineIdx, pageIdx, canvas.toLayoutX(x), canvas.toLayoutY(y))
            if (hit.coordinate.spineIdx != spineIdx) return
            val word = clamp(session.wordAt(hit.coordinate), pageRange)
            if (word.start >= word.end) return
            val rects = session.selectionRects(spineIdx, word)
            if (rects.isEmpty()) return
            active = ActiveSelection(spineIdx, pageIdx, pageRange, word)
            overlay.show(rects, canvas.palette.link)
            canvas.performHapticFeedback(HapticFeedbackConstants.LONG_PRESS)
            presentActionMode()
        } catch (_: InkunaException) {
            // Published geometry can be invalidated between down and long press.
        }
    }

    fun clear() {
        cancelSearchHighlight()
        dragging = null
        active = null
        overlay.clear()
        actionMode?.finish()
        actionMode = null
    }

    fun containsSelection(x: Float, y: Float): Boolean = overlay.containsHighlight(x, y)

    fun updatePalette(accent: Int) {
        overlay.updateAccent(accent)
    }

    fun showSearchHighlight(rects: List<SelectionRect>) {
        clear()
        if (rects.isEmpty()) return

        overlay.showSearchHighlight(rects, searchHighlightAccent)
        searchHighlightToken += 1
        val token = searchHighlightToken
        val dismissal = Runnable {
            overlay.animate()
                .alpha(0f)
                .setDuration(SelectionOverlay.searchHighlightFade)
                .withEndAction {
                    if (searchHighlightToken == token && !isActive) {
                        overlay.alpha = 1f
                        overlay.clear()
                        searchHighlightDismissal = null
                    }
                }
                .start()
        }
        searchHighlightDismissal = dismissal
        overlay.postDelayed(dismissal, SelectionOverlay.searchHighlightHold)
    }

    private fun cancelSearchHighlight() {
        searchHighlightToken += 1
        searchHighlightDismissal?.let(overlay::removeCallbacks)
        searchHighlightDismissal = null
        overlay.animate().cancel()
        overlay.alpha = 1f
    }

    private fun beginHandleDrag(handle: Handle) {
        val selection = active ?: return
        dragging = HandleDrag(
            anchor = if (handle == Handle.START) selection.range.end else selection.range.start,
        )
        actionMode?.finish()
        actionMode = null
    }

    private fun updateHandle(x: Float, y: Float) {
        val selection = active ?: return
        val drag = dragging ?: return
        try {
            val hit = session.hitTest(
                selection.spineIdx,
                selection.pageIdx,
                canvas.toLayoutX(x),
                canvas.toLayoutY(y),
            )
            if (hit.coordinate.spineIdx != selection.spineIdx) return
            val boundary = hit.coordinate.charOffset.coerceIn(selection.pageRange.start, selection.pageRange.end)
            if (boundary == drag.anchor) return
            val range = CharRange(minOf(boundary, drag.anchor), maxOf(boundary, drag.anchor))
            val rects = session.selectionRects(selection.spineIdx, range)
            if (rects.isEmpty()) return
            selection.range = range
            overlay.show(rects, canvas.palette.link)
        } catch (_: InkunaException) {
            // Keep the last valid highlight instead of flickering on a stale snapshot.
        }
    }

    private fun presentActionMode() {
        if (active == null || overlay.bounds.isEmpty) return
        actionMode = canvas.startActionMode(object : ActionMode.Callback2() {
            override fun onCreateActionMode(mode: ActionMode, menu: android.view.Menu): Boolean {
                menu.add(0, COPY, 0, android.R.string.copy)
                menu.add(0, SHARE, 1, R.string.reader_share_selection)
                menu.add(0, WEB_SEARCH, 2, R.string.reader_web_search)
                return true
            }

            override fun onPrepareActionMode(mode: ActionMode, menu: android.view.Menu) = false

            override fun onActionItemClicked(mode: ActionMode, item: android.view.MenuItem): Boolean = when (item.itemId) {
                COPY -> selectedText()?.let(::copy) != null
                SHARE -> selectedText()?.let(::share) != null
                WEB_SEARCH -> selectedText()?.let(::webSearch) != null
                else -> false
            }

            override fun onDestroyActionMode(mode: ActionMode) {
                if (actionMode === mode) actionMode = null
            }

            override fun onGetContentRect(mode: ActionMode, view: View, outRect: Rect) {
                val rect = overlay.bounds
                outRect.set(rect.left.toInt(), rect.top.toInt(), rect.right.toInt(), rect.bottom.toInt())
            }
        }, ActionMode.TYPE_FLOATING)
    }

    private fun selectedText(): String? = active?.let { selection ->
        runCatching { session.textRange(selection.spineIdx, selection.range) }.getOrNull()
    }?.takeIf { it.isNotEmpty() }

    private fun copy(text: String) {
        canvas.context.getSystemService(ClipboardManager::class.java)
            ?.setPrimaryClip(ClipData.newPlainText("selection", text))
    }

    private fun share(text: String) {
        canvas.context.startActivity(
            Intent.createChooser(Intent(Intent.ACTION_SEND).apply {
                type = "text/plain"
                putExtra(Intent.EXTRA_TEXT, text)
            }, null).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
        )
    }

    private fun webSearch(text: String) {
        canvas.context.startActivity(Intent(Intent.ACTION_WEB_SEARCH).apply {
            putExtra("query", text)
            addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        })
    }

    private fun clamp(range: CharRange, page: CharRange) = CharRange(
        range.start.coerceIn(page.start, page.end),
        range.end.coerceIn(page.start, page.end),
    )

    private val searchHighlightAccent: Int
        get() = if (canvas.palette.link == SelectionOverlay.searchHighlightNightColor) {
            SelectionOverlay.searchHighlightNightColor
        } else {
            SelectionOverlay.searchHighlightDayColor
        }

    private class SelectionOverlay(
        context: Context,
        private val onHandle: (Handle, Int, Float, Float) -> Unit,
    ) : View(context) {
        private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
        private var rects: List<RectF> = emptyList()
        private var mode = WritingMode.HORIZONTAL_TB
        private var accent = 0
        private var handle: Handle? = null
        private var presentation = Presentation.SELECTION

        val bounds: RectF get() = rects.fold(RectF()) { total, rect ->
            if (total.isEmpty) RectF(rect) else total.apply { union(rect) }
        }

        init {
            isClickable = true
        }

        fun show(source: List<SelectionRect>, color: Int) {
            presentation = Presentation.SELECTION
            present(source, color)
        }

        fun showSearchHighlight(source: List<SelectionRect>, color: Int) {
            presentation = Presentation.SEARCH_HIGHLIGHT
            present(source, color)
        }

        private fun present(source: List<SelectionRect>, color: Int) {
            rects = source.mapNotNull { selection ->
                val rect = selection.rect
                val scaled = RectF(
                    (rect.x * resources.displayMetrics.density).toFloat(),
                    (rect.y * resources.displayMetrics.density).toFloat(),
                    ((rect.x + rect.width) * resources.displayMetrics.density).toFloat(),
                    ((rect.y + rect.height) * resources.displayMetrics.density).toFloat(),
                )
                scaled.takeIf { !it.isEmpty }
            }
            mode = source.firstOrNull()?.writingMode ?: WritingMode.HORIZONTAL_TB
            accent = color
            visibility = if (rects.isEmpty()) GONE else VISIBLE
            invalidate()
        }

        fun clear() {
            handle = null
            presentation = Presentation.SELECTION
            rects = emptyList()
            visibility = GONE
            invalidate()
        }

        fun updateAccent(color: Int) {
            accent = color
            invalidate()
        }

        fun containsHighlight(x: Float, y: Float) = rects.any { it.contains(x, y) }

        override fun onDraw(canvas: Canvas) {
            if (rects.isEmpty()) return
            paint.style = Paint.Style.FILL
            when (presentation) {
                Presentation.SELECTION -> {
                    paint.color = (accent and 0x00FFFFFF) or 0x4D000000
                    rects.forEach { rect -> canvas.drawRect(rect, paint) }
                }
                Presentation.SEARCH_HIGHLIGHT -> {
                    paint.color = (accent and 0x00FFFFFF) or
                        ((searchHighlightAlpha * 255f).roundToInt() shl 24)
                    val radius = searchHighlightCornerRadius * resources.displayMetrics.density
                    rects.forEach { rect -> canvas.drawRoundRect(rect, radius, radius, paint) }
                    return
                }
            }
            paint.color = accent
            handleGeometry(Handle.START)?.let { drawTeardrop(canvas, it.first, it.second) }
            handleGeometry(Handle.END)?.let { drawTeardrop(canvas, it.first, it.second) }
        }

        override fun onTouchEvent(event: MotionEvent): Boolean {
            if (presentation != Presentation.SELECTION) return false
            when (event.actionMasked) {
                MotionEvent.ACTION_DOWN -> {
                    handle = listOf(Handle.START, Handle.END).firstOrNull { candidate ->
                        handleGeometry(candidate)?.second?.let { point ->
                            val radius = HANDLE * 1.8f
                            point.x in event.x - radius..event.x + radius && point.y in event.y - radius..event.y + radius
                        } == true
                    }
                    val selected = handle ?: return false
                    onHandle(selected, event.actionMasked, event.x, event.y)
                    return true
                }
                MotionEvent.ACTION_MOVE, MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                    val selected = handle ?: return false
                    onHandle(selected, event.actionMasked, event.x, event.y)
                    if (event.actionMasked != MotionEvent.ACTION_MOVE) handle = null
                    return true
                }
            }
            return false
        }

        private fun handleGeometry(handle: Handle): Pair<android.graphics.PointF, android.graphics.PointF>? {
            val first = rects.firstOrNull() ?: return null
            val last = rects.lastOrNull() ?: return null
            val anchor = when (mode to handle) {
                WritingMode.HORIZONTAL_TB to Handle.START -> android.graphics.PointF(first.left, first.top)
                WritingMode.HORIZONTAL_TB to Handle.END -> android.graphics.PointF(last.right, last.bottom)
                WritingMode.VERTICAL_RL to Handle.START -> android.graphics.PointF(first.right, first.top)
                WritingMode.VERTICAL_RL to Handle.END -> android.graphics.PointF(last.left, last.bottom)
                else -> return null
            }
            val knob = when (mode to handle) {
                WritingMode.HORIZONTAL_TB to Handle.START -> android.graphics.PointF(anchor.x, anchor.y - HANDLE)
                WritingMode.HORIZONTAL_TB to Handle.END -> android.graphics.PointF(anchor.x, anchor.y + HANDLE)
                WritingMode.VERTICAL_RL to Handle.START -> android.graphics.PointF(anchor.x + HANDLE, anchor.y)
                WritingMode.VERTICAL_RL to Handle.END -> android.graphics.PointF(anchor.x - HANDLE, anchor.y)
                else -> anchor
            }
            return anchor to knob
        }

        private fun drawTeardrop(canvas: Canvas, anchor: android.graphics.PointF, knob: android.graphics.PointF) {
            paint.strokeWidth = 2f * resources.displayMetrics.density
            canvas.drawLine(anchor.x, anchor.y, knob.x, knob.y, paint)
            canvas.drawCircle(knob.x, knob.y, HANDLE / 2f, paint)
        }

        companion object {
            const val HANDLE = 24f
            val searchHighlightDayColor = 0xFFB4863B.toInt()
            val searchHighlightNightColor = 0xFFD9AE63.toInt()
            const val searchHighlightAlpha = 0.35f
            const val searchHighlightCornerRadius = 3f
            const val searchHighlightHold = 1_500L
            const val searchHighlightFade = 600L
        }

        private enum class Presentation { SELECTION, SEARCH_HIGHLIGHT }
    }

    private companion object {
        const val COPY = 1
        const val SHARE = 2
        const val WEB_SEARCH = 3
    }
}
