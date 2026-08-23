package app.inkuna.android.ui.reader.engine

import app.inkuna.android.ui.reader.ReaderPagerStrip
import app.inkuna.android.ui.reader.ReaderPagerSurface
import app.inkuna.core.ChapterGeometry
import app.inkuna.core.InkunaException
import app.inkuna.core.ReaderSession
import kotlin.math.roundToInt

sealed interface ChapterReadiness {
    data object Empty : ChapterReadiness
    data class Partial(val publishedPages: UInt) : ChapterReadiness
    data class Complete(val geometry: ChapterGeometry) : ChapterReadiness
}

/** Engine-backed pager state. Every session read is synchronous and cache-only. */
class EnginePagerSurface(
    private val session: ReaderSession,
    private val canvas: EnginePageCanvas,
) : ReaderPagerSurface {
    private data class NeighborKey(val spineIdx: UInt, val toRight: Boolean)

    private val readiness = mutableMapOf<UInt, ChapterReadiness>()
    private val failedSpines = mutableSetOf<UInt>()
    private val neighborReadiness = mutableMapOf<NeighborKey, Boolean>()
    private var latestGeneration: ULong? = null
    private var pendingGeneration = false
    private var innerOffset = 0f
    private var outerDisplacement = 0f
    private var scenePageCount = 0u
    private var interactionPageCount: UInt? = null
    private var lastSettledPage: UInt? = null

    var spineIdx: UInt = 0u
        private set
    var pageIdx: UInt = 0u
        private set
    var spineCount: UInt = session.spineCount()
    var onPageSettled: ((UInt, UInt) -> Unit)? = null
    var selectionActive: Boolean = false

    init {
        canvas.attach(session)
    }

    override val isEngageable: Boolean
        get() = !isBusy && canvas.width > 0 && canvas.height > 0 &&
            (spineIdx in failedSpines || canvas.hasPage(spineIdx, pageIdx))
    override val isBusy: Boolean get() = pendingGeneration
    override val hasActiveSelection: Boolean get() = selectionActive
    override val isRightToLeft: Boolean get() = session.isRtl()

    fun display(spineIdx: UInt, pageIdx: UInt) {
        this.spineIdx = spineIdx
        this.pageIdx = pageIdx
        interactionPageCount = null
        primeReadiness(spineIdx)
        val count = pageCountFor(spineIdx)
        innerOffset = PageSlot.offset(pageIdx, count, isRightToLeft, pageWidth)
        outerDisplacement = 0f
        scenePageCount = count
        lastSettledPage = pageIdx
        neighborReadiness.clear()
        canvas.showUnreadablePlaceholder(spineIdx in failedSpines)
        setScene()
        onPageSettled?.invoke(spineIdx, pageIdx)
    }

    fun firstPageBecameReady(generation: ULong, spineIdx: UInt) {
        if (!accept(generation)) return
        val published = maxOf(1u, session.publishedPageCount(spineIdx))
        if ((readiness[spineIdx] as? ChapterReadiness.Complete)?.geometry?.generation != generation) {
            readiness[spineIdx] = ChapterReadiness.Partial(published)
        }
        failedSpines.remove(spineIdx)
        neighborReadiness.clear()
        if (spineIdx == this.spineIdx) {
            pendingGeneration = false
            canvas.showUnreadablePlaceholder(false)
            canvas.invalidate(generation)
            setScene()
        }
    }

    fun chapterBecameReady(generation: ULong, spineIdx: UInt) {
        if (!accept(generation)) return
        val geometry = try {
            session.chapter(spineIdx)
        } catch (_: InkunaException.NotReady) {
            return
        } catch (_: InkunaException.UnsupportedContent) {
            return
        }
        if (geometry.generation != generation) return
        readiness[spineIdx] = ChapterReadiness.Complete(geometry)
        failedSpines.remove(spineIdx)
        neighborReadiness.clear()
        if (spineIdx == this.spineIdx) {
            pendingGeneration = false
            canvas.showUnreadablePlaceholder(false)
            canvas.invalidate(generation)
            setScene()
        }
    }

    fun chapterFailed(generation: ULong, spineIdx: UInt) {
        if (!accept(generation)) return
        readiness[spineIdx] = ChapterReadiness.Empty
        failedSpines += spineIdx
        neighborReadiness.clear()
        if (spineIdx == this.spineIdx) {
            pendingGeneration = false
            canvas.showUnreadablePlaceholder(true)
            setScene()
        }
    }

    fun layoutInvalidated(generation: ULong) {
        latestGeneration = generation.takeIf { it != 0uL }
        pendingGeneration = true
        readiness.clear()
        failedSpines.clear()
        neighborReadiness.clear()
        scenePageCount = 0u
        interactionPageCount = null
        lastSettledPage = null
        latestGeneration?.let(canvas::invalidate) ?: canvas.invalidateAll()
    }

    override fun beginPagingInteraction() {
        val count = pageCountFor(spineIdx)
        rebaseInnerOffset(count)
        interactionPageCount = count
    }

    override fun endPagingInteraction() {
        interactionPageCount = null
        setScene()
    }

    override fun innerMetrics(): ReaderPagerStrip? {
        val width = pageWidth
        if (width <= 0f) return null
        val count = displayedPageCount()
        rebaseInnerOffset(count)
        val end = (maxOf(1u, count) - 1u).toFloat() * width
        return when (val state = readiness[spineIdx] ?: ChapterReadiness.Empty) {
            is ChapterReadiness.Complete -> {
                if (state.geometry.generation != latestGeneration) return null
                ReaderPagerStrip(innerOffset, 0f..end, width)
            }
            is ChapterReadiness.Partial ->
                ReaderPagerStrip(innerOffset, 0f..maxOf(innerOffset, end), width)
            ChapterReadiness.Empty -> if (spineIdx in failedSpines) {
                ReaderPagerStrip(innerOffset, 0f..0f, width)
            } else {
                null
            }
        }
    }

    override fun setInnerOffset(x: Float) {
        val metrics = innerMetrics() ?: return
        innerOffset = x.coerceIn(metrics.range.start, metrics.range.endInclusive)
        setScene()
        val count = displayedPageCount()
        val slot = (innerOffset / metrics.pageWidth).roundToInt().toUInt()
        val settled = PageSlot.pageIdx(slot, count, isRightToLeft)
        val settledOffset = PageSlot.offset(settled, count, isRightToLeft, metrics.pageWidth)
        if (kotlin.math.abs(innerOffset - settledOffset) < 0.01f && settled != lastSettledPage) {
            pageIdx = settled
            lastSettledPage = settled
            onPageSettled?.invoke(spineIdx, settled)
        }
    }

    override fun outerMetrics(): ReaderPagerStrip? {
        val width = pageWidth
        if (width <= 0f) return null
        val leftExists = neighborSpine(toRight = false) != null
        val rightExists = neighborSpine(toRight = true) != null
        return ReaderPagerStrip(
            offset = width - outerDisplacement,
            range = (if (leftExists) 0f else width)..(if (rightExists) width * 2f else width),
            pageWidth = width,
        )
    }

    override fun setOuterOffset(x: Float) {
        val metrics = outerMetrics() ?: return
        val offset = x.coerceIn(metrics.range.start, metrics.range.endInclusive)
        outerDisplacement = metrics.pageWidth - offset
        setScene()
    }

    override fun neighborIsReady(toRight: Boolean): Boolean {
        val neighbor = neighborSpine(toRight) ?: return false
        val key = NeighborKey(neighbor, toRight)
        neighborReadiness[key]?.let { return it }
        val ready = when {
            // A failed chapter occupies one placeholder page; it must stay
            // crossable in both directions or every chapter beyond it
            // becomes unreachable by paging.
            neighbor in failedSpines -> true
            isForward(toRight) -> {
                val published = when (readiness[neighbor]) {
                    is ChapterReadiness.Partial, is ChapterReadiness.Complete -> true
                    ChapterReadiness.Empty, null -> session.publishedPageCount(neighbor) > 0u
                }
                // A cache miss schedules the chapter, so a later layout
                // event resolves the crossing (mirrors iOS).
                if (!published) runCatching { session.chapter(neighbor) }
                published
            }
            else -> completeGeometry(neighbor) != null
        }
        neighborReadiness[key] = ready
        return ready
    }

    override fun commitBoundaryCrossing(toRight: Boolean): Boolean {
        val target = neighborSpine(toRight) ?: return false
        if (!neighborIsReady(toRight)) return false
        val targetPage = when {
            target in failedSpines -> 0u
            isForward(toRight) -> 0u
            else -> {
                val geometry = completeGeometry(target) ?: return false
                geometry.pageCount - 1u
            }
        }
        spineIdx = target
        pageIdx = targetPage
        val count = pageCountFor(target)
        innerOffset = PageSlot.offset(targetPage, count, isRightToLeft, pageWidth)
        scenePageCount = count
        interactionPageCount = count
        outerDisplacement = 0f
        lastSettledPage = targetPage
        neighborReadiness.clear()
        canvas.showUnreadablePlaceholder(target in failedSpines)
        setScene()
        onPageSettled?.invoke(target, targetPage)
        return true
    }

    private val pageWidth: Float get() = canvas.width.toFloat()

    private fun accept(generation: ULong): Boolean {
        val known = latestGeneration
        if (known != null && known != generation) return false
        if (known == null) {
            latestGeneration = generation
            canvas.invalidate(generation)
        }
        return true
    }

    /**
     * Learns a spine's readiness from the session cache when no layout
     * event has taught it yet — a display can land before the event
     * stream reaches this surface (fresh mount, replayed-out events).
     * Without a page count the strip would anchor every page at slot 0.
     */
    private fun primeReadiness(spineIdx: UInt) {
        if (readiness[spineIdx] != null || spineIdx in failedSpines) return
        val published = session.publishedPageCount(spineIdx)
        if (published > 0u) readiness[spineIdx] = ChapterReadiness.Partial(published)
    }

    private fun pageCountFor(spineIdx: UInt): UInt {
        if (spineIdx in failedSpines) return 1u
        return when (val state = readiness[spineIdx]) {
            is ChapterReadiness.Complete -> state.geometry.pageCount
            is ChapterReadiness.Partial -> maxOf(state.publishedPages, session.publishedPageCount(spineIdx))
            ChapterReadiness.Empty, null -> 0u
        }
    }

    /**
     * Complete geometry with at least one page, healing the readiness map
     * from the session cache (a replayed event stream may be shorter than
     * the book). A miss schedules the chapter and answers null.
     */
    private fun completeGeometry(spineIdx: UInt): ChapterGeometry? {
        (readiness[spineIdx] as? ChapterReadiness.Complete)?.geometry
            ?.takeIf { it.pageCount > 0u }
            ?.let { return it }
        val geometry = runCatching { session.chapter(spineIdx) }.getOrNull() ?: return null
        if (latestGeneration != null && geometry.generation != latestGeneration) return null
        if (geometry.pageCount == 0u) return null
        readiness[spineIdx] = ChapterReadiness.Complete(geometry)
        return geometry
    }

    private fun setScene() {
        val count = displayedPageCount()
        rebaseInnerOffset(count)
        val edge = when {
            outerDisplacement == 0f -> null
            else -> {
                val toRight = outerDisplacement < 0f
                neighborEntry(toRight)
            }
        }
        canvas.setScene(
            PageScene(spineIdx, count, isRightToLeft, innerOffset, outerDisplacement, edge),
        )
    }

    private fun rebaseInnerOffset(pageCount: UInt) {
        if (isRightToLeft && scenePageCount > 0u && pageCount > scenePageCount) {
            innerOffset += (pageCount - scenePageCount).toFloat() * pageWidth
        }
        scenePageCount = pageCount
    }

    /**
     * The strip's page count. During a paging interaction it is frozen so
     * layout events cannot remap slots under the finger — except that in
     * LTR progression newly published pages append past the strip's end
     * without moving any existing offset, so growth is adopted live and a
     * drag can reach pages published during it. In RTL, growth would
     * rebase every offset; it stays deferred to [endPagingInteraction].
     */
    private fun displayedPageCount(): UInt {
        val frozen = interactionPageCount ?: return pageCountFor(spineIdx)
        if (isRightToLeft) return frozen
        val live = pageCountFor(spineIdx)
        if (live <= frozen) return frozen
        interactionPageCount = live
        return live
    }

    private fun neighborSpine(toRight: Boolean): UInt? {
        val delta = if (toRight) {
            if (isRightToLeft) -1 else 1
        } else {
            if (isRightToLeft) 1 else -1
        }
        val candidate = spineIdx.toLong() + delta
        return candidate.takeIf { it >= 0 && it < spineCount.toLong() }?.toUInt()
    }

    private fun isForward(toRight: Boolean): Boolean = toRight != isRightToLeft

    private fun neighborEntry(toRight: Boolean): PageNeighbor? {
        val neighbor = neighborSpine(toRight) ?: return null
        if (!neighborIsReady(toRight)) return null
        val page = when {
            neighbor in failedSpines -> 0u
            isForward(toRight) -> 0u
            else -> {
                val geometry = completeGeometry(neighbor) ?: return null
                geometry.pageCount - 1u
            }
        }
        return PageNeighbor(neighbor, page, toRight)
    }
}
