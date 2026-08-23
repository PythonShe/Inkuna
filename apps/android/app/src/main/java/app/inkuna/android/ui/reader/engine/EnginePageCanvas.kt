package app.inkuna.android.ui.reader.engine

import android.content.Context
import android.view.GestureDetector
import android.view.MotionEvent
import android.view.View
import android.widget.FrameLayout
import android.widget.TextView
import android.view.Gravity
import app.inkuna.core.PageDisplayList
import app.inkuna.core.ReaderSession
import app.inkuna.core.InkunaException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlin.math.floor

/** The current engine strip; pager offsets are view pixels, page geometry is dp. */
data class PageScene(
    val spineIdx: UInt,
    val pageCount: UInt,
    val rtl: Boolean,
    val innerOffset: Float,
    val outerDisplacement: Float,
    val neighborEdge: PageNeighbor? = null,
)

data class PageNeighbor(
    val spineIdx: UInt,
    val pageIdx: UInt,
    val toRight: Boolean,
)

/** The sole logical-page to physical-strip-slot conversion for the engine canvas. */
internal object PageSlot {
    fun slot(pageIdx: UInt, pageCount: UInt, rtl: Boolean): UInt = when {
        pageIdx >= pageCount -> 0u
        rtl -> pageCount - pageIdx - 1u
        else -> pageIdx
    }

    fun pageIdx(slot: UInt, pageCount: UInt, rtl: Boolean): UInt = when {
        slot >= pageCount -> 0u
        rtl -> pageCount - slot - 1u
        else -> slot
    }

    fun offset(pageIdx: UInt, pageCount: UInt, rtl: Boolean, pageWidth: Float): Float =
        slot(pageIdx, pageCount, rtl).toFloat() * pageWidth
}

/** Mounts and positions the small visible window of native engine pages. */
class EnginePageCanvas(context: Context) : FrameLayout(context) {
    private data class PageKey(val spineIdx: UInt, val pageIdx: UInt)

    private data class MountedPage(
        val view: PageView,
        var generation: ULong?,
        var queried: Boolean,
        var use: ULong,
    )

    private var scope = newScope()
    private val mounted = mutableMapOf<PageKey, MountedPage>()
    private var session: ReaderSession? = null
    private var imageLoader: PageImageLoader? = null
    private var scene: PageScene? = null
    private var latestGeneration: ULong? = null
    private var useCounter = 0uL
    private var unreadableView: TextView? = null
    private var selectionOverlay: View? = null
    internal var selectionController: ReaderSelectionController? = null
    private var gestureSpineIdx = 0u
    private var gesturePageIdx = 0u
    private val gestures = GestureDetector(context, object : GestureDetector.SimpleOnGestureListener() {
        override fun onDown(event: MotionEvent) = true

        override fun onSingleTapUp(event: MotionEvent): Boolean {
            if (scene == null) return false
            if (selectionController?.isActive == true) {
                if (!selectionController!!.containsSelection(event.x, event.y)) selectionController?.clear()
            } else {
                onPageTap?.invoke(gestureSpineIdx, gesturePageIdx, event.x, event.y)
            }
            return true
        }

        override fun onLongPress(event: MotionEvent) {
            selectionController?.beginAt(gestureSpineIdx, gesturePageIdx, event.x, event.y)
        }
    })

    var palette: PagePalette = PagePalette.from(app.inkuna.android.ui.theme.ReadingTheme.Paper)
        set(value) {
            field = value
            mounted.values.forEach { it.view.palette = value }
            setBackgroundColor(value.background)
            unreadableView?.setTextColor(value.secondary)
            selectionController?.updatePalette(value.link)
        }

    var onLinkActivated: ((spineIdx: UInt, pageIdx: UInt, x: Float, y: Float) -> Unit)? = null
    var onPageTap: ((spineIdx: UInt, pageIdx: UInt, x: Float, y: Float) -> Unit)? = null
    var onPageDrawn: ((spineIdx: UInt, pageIdx: UInt) -> Unit)? = null

    init {
        setBackgroundColor(palette.background)
        clipChildren = true
    }

    internal fun attach(session: ReaderSession) {
        this.session = session
        imageLoader = PageImageLoader(session) { scope }
        updatePages()
    }

    internal fun installSelectionOverlay(overlay: View) {
        selectionOverlay?.let(::removeView)
        selectionOverlay = overlay
        addView(overlay, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
        bringChildToFront(overlay)
    }

    internal fun showUnreadablePlaceholder(show: Boolean) {
        val placeholder = unreadableView ?: TextView(context).also { view ->
            view.gravity = Gravity.CENTER
            view.text = context.getString(app.inkuna.android.R.string.reader_chapter_unreadable)
            addView(view, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
            unreadableView = view
        }
        placeholder.setTextColor(palette.secondary)
        placeholder.visibility = if (show) View.VISIBLE else View.GONE
        if (show) {
            mounted.values.forEach { it.view.visibility = View.INVISIBLE }
        } else {
            updatePages()
        }
        selectionOverlay?.let(::bringChildToFront)
    }

    internal fun toLayoutX(x: Float): Double = (x / resources.displayMetrics.density).toDouble()

    internal fun toLayoutY(y: Float): Double = (y / resources.displayMetrics.density).toDouble()

    private fun receivePageTouch(spineIdx: UInt, pageIdx: UInt, event: MotionEvent): Boolean {
        gestureSpineIdx = spineIdx
        gesturePageIdx = pageIdx
        gestures.onTouchEvent(event)
        if (event.actionMasked == MotionEvent.ACTION_CANCEL) gestures.onTouchEvent(event)
        return true
    }

    fun setScene(scene: PageScene) {
        this.scene = scene
        updatePages()
    }

    /** Discards lists from older engine generations and re-queries only the visible window. */
    fun invalidate(generation: ULong) {
        latestGeneration = generation
        val stale = mounted.filterValues { it.generation != generation }.keys.toList()
        stale.forEach { key ->
            mounted.remove(key)?.view?.let(::removeView)
        }
        mounted.values.filter { it.generation == null }.forEach { it.queried = false }
        updatePages()
    }

    internal fun invalidateAll() {
        latestGeneration = null
        mounted.values.forEach { removeView(it.view) }
        mounted.clear()
        updatePages()
    }

    internal fun hasPage(spineIdx: UInt, pageIdx: UInt): Boolean {
        val generation = latestGeneration ?: return false
        return mounted[PageKey(spineIdx, pageIdx)]?.generation == generation
    }

    override fun onLayout(changed: Boolean, left: Int, top: Int, right: Int, bottom: Int) {
        super.onLayout(changed, left, top, right, bottom)
        updatePages()
    }

    override fun onDetachedFromWindow() {
        scope.cancel()
        super.onDetachedFromWindow()
    }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        if (scope.coroutineContext[Job]?.isActive != true) scope = newScope()
    }

    private fun updatePages() {
        val currentScene = scene ?: return
        if (width <= 0 || height <= 0) return
        val pageWidth = width.toFloat()
        val pageHeight = height
        val targets = mutableListOf<Pair<PageKey, Float>>()

        if (currentScene.pageCount > 0u) {
            val contentOffset = currentScene.innerOffset - currentScene.outerDisplacement
            val firstSlot = maxOf(0, floor(contentOffset / pageWidth).toInt())
            val lastSlot = minOf(
                currentScene.pageCount.toInt() - 1,
                floor((contentOffset + pageWidth - Float.MIN_VALUE) / pageWidth).toInt(),
            )
            if (firstSlot <= lastSlot) {
                for (slot in firstSlot..lastSlot) {
                    val pageIdx = PageSlot.pageIdx(slot.toUInt(), currentScene.pageCount, currentScene.rtl)
                    val x = slot * pageWidth - currentScene.innerOffset + currentScene.outerDisplacement
                    if (x < pageWidth && x + pageWidth > 0f) {
                        targets += PageKey(currentScene.spineIdx, pageIdx) to x
                    }
                }
            }
        }

        currentScene.neighborEdge?.let { edge ->
            val x = (if (edge.toRight) pageWidth else -pageWidth) + currentScene.outerDisplacement
            if (x < pageWidth && x + pageWidth > 0f) {
                targets += PageKey(edge.spineIdx, edge.pageIdx) to x
            }
        }

        val targetKeys = targets.mapTo(mutableSetOf()) { it.first }
        mounted.forEach { (key, page) ->
            if (key !in targetKeys) page.view.visibility = View.INVISIBLE
        }
        targets.forEach { (key, x) ->
            val page = mount(key)
            val params = page.view.layoutParams as? LayoutParams
            if (params?.width != width || params.height != pageHeight) {
                page.view.layoutParams = LayoutParams(width, pageHeight)
            }
            page.view.layout(0, 0, width, pageHeight)
            page.view.translationX = x
            page.view.translationY = 0f
            page.view.visibility = if (unreadableView?.visibility == View.VISIBLE) View.INVISIBLE else View.VISIBLE
        }
        selectionOverlay?.let(::bringChildToFront)
    }

    private fun mount(key: PageKey): MountedPage {
        useCounter += 1uL
        mounted[key]?.let { existing ->
            existing.use = useCounter
            if (existing.generation == null && !existing.queried) {
                existing.queried = true
                displayListFor(key)?.let { list ->
                    existing.generation = list.generation
                    existing.view.present(list, key.spineIdx, key.pageIdx, session ?: return existing)
                }
            }
            return existing
        }

        val view = if (mounted.size >= PAGE_POOL_SIZE) {
            val victim = mounted.minByOrNull { it.value.use }!!
            mounted.remove(victim.key)!!.view
        } else {
            PageView(context).also { addView(it) }
        }
        view.palette = palette
        view.imageLoader = imageLoader
        view.onLinkActivated = { spineIdx, pageIdx, x, y ->
            onLinkActivated?.invoke(spineIdx, pageIdx, x, y)
        }
        view.setOnTouchListener { _, event -> receivePageTouch(key.spineIdx, key.pageIdx, event) }
        view.onPageDrawn = { spineIdx, pageIdx -> onPageDrawn?.invoke(spineIdx, pageIdx) }
        val list = displayListFor(key)
        view.present(
            list,
            key.spineIdx,
            key.pageIdx,
            session ?: return MountedPage(view, null, queried = true, use = useCounter),
        )
        return MountedPage(view, list?.generation, queried = true, use = useCounter)
            .also { mounted[key] = it }
    }

    private fun displayListFor(key: PageKey): PageDisplayList? {
        val expectedGeneration = latestGeneration ?: return null
        val list = try {
            session?.page(key.spineIdx, key.pageIdx)
        } catch (_: InkunaException.NotReady) {
            null
        } catch (_: InkunaException.UnsupportedContent) {
            null
        } ?: return null
        return list.takeIf { it.generation == expectedGeneration }
    }

    private companion object {
        const val PAGE_POOL_SIZE = 6
    }

    private fun newScope() = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
}
