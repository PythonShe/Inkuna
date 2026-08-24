package app.inkuna.android.ui.reader.engine

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.RectF
import android.view.View
import androidx.core.view.ViewCompat
import androidx.compose.ui.graphics.toArgb
import app.inkuna.android.ui.theme.ReadingTheme
import app.inkuna.core.A11yBlock
import app.inkuna.core.ColorRole
import app.inkuna.core.Decoration
import app.inkuna.core.ImagePlacement
import app.inkuna.core.PageDisplayList
import app.inkuna.core.ReaderSession
import app.inkuna.core.RunOrientation
import kotlin.math.min

/** Palette roles assigned by the core and resolved through the reader theme. */
data class PagePalette(
    val background: Int,
    val text: Int,
    val secondary: Int,
    val link: Int,
) {
    companion object {
        fun from(theme: ReadingTheme): PagePalette = PagePalette(
            background = theme.background.toArgb(),
            text = theme.foreground.toArgb(),
            secondary = theme.dimmed.toArgb(),
            link = if (theme.isNight) 0xFFD9AE63.toInt() else 0xFFB4863B.toInt(),
        )
    }
}

/** A static native rendering surface for one engine page display list. */
class PageView(context: Context) : View(context) {
    var palette: PagePalette = PagePalette.from(ReadingTheme.Paper)
        set(value) {
            field = value
            invalidate()
        }

    var imageLoader: PageImageLoader? = null

    /** Set by the canvas; Accessibility links enter the same page-local path. */
    var onLinkActivated: ((spineIdx: UInt, pageIdx: UInt, x: Float, y: Float) -> Unit)? = null
    var onPageDrawn: ((spineIdx: UInt, pageIdx: UInt) -> Unit)? = null

    private val density = resources.displayMetrics.density
    private val glyphPaint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val imagePaint = Paint(Paint.ANTI_ALIAS_FLAG or Paint.FILTER_BITMAP_FLAG)
    private val accessibilityHelper = PageAccessibilityHelper(this)

    private var displayList: PageDisplayList? = null
    private var renderedRuns: List<RenderedGlyphRun> = emptyList()
    private var spineIdx = 0u
    private var pageIdx = 0u
    private var session: ReaderSession? = null

    init {
        ViewCompat.setAccessibilityDelegate(this, accessibilityHelper)
    }

    /** Presents a fully materialized display list; primitive arrays are built once here. */
    fun present(list: PageDisplayList?, spineIdx: UInt, pageIdx: UInt, session: ReaderSession) {
        displayList = list
        renderedRuns = list?.glyphRuns.orEmpty().mapNotNull(::renderedRun)
        this.spineIdx = spineIdx
        this.pageIdx = pageIdx
        this.session = session
        accessibilityHelper.onPageContentChanged()
        invalidate()
    }

    override fun dispatchHoverEvent(event: android.view.MotionEvent): Boolean =
        accessibilityHelper.dispatchHoverEvent(event) || super.dispatchHoverEvent(event)

    override fun onDraw(canvas: Canvas) {
        canvas.drawColor(palette.background)
        val list = displayList ?: return

        canvas.save()
        canvas.scale(density, density)
        drawImages(canvas, list.images)
        drawGlyphRuns(canvas)
        drawDecorations(canvas, list.decorations)
        canvas.restore()
        // A page whose runs were dropped for want of faces has not really
        // rendered; the store's revision brings us back here once it has.
        if (renderedRuns.isEmpty() || ReaderFontStore.isPrimed) onPageDrawn?.invoke(spineIdx, pageIdx)
    }

    internal fun accessibilityBlocks(): List<A11yBlock> = displayList?.a11y.orEmpty()

    /**
     * Emits the block's centre in the core's page-local layout points —
     * the space `A11yBlock.rect` and `hitTest` both speak — never view
     * pixels. Its consumer must not re-divide by the display density.
     */
    internal fun activateLink(block: A11yBlock): Boolean {
        if (!block.isLink) return false
        onLinkActivated?.invoke(
            spineIdx,
            pageIdx,
            (block.rect.x + block.rect.width / 2.0).toFloat(),
            (block.rect.y + block.rect.height / 2.0).toFloat(),
        )
        return true
    }

    private fun drawGlyphRuns(canvas: Canvas) {
        renderedRuns.forEach { run ->
            val font = ReaderFontStore.font(run.fontId) ?: return@forEach
            glyphPaint.textSize = run.size
            glyphPaint.color = colorFor(run.colorRole)
            when (run.orientation) {
                RunOrientation.UPRIGHT -> canvas.drawGlyphs(
                    run.glyphIds,
                    0,
                    run.positions,
                    0,
                    run.glyphIds.size,
                    font,
                    glyphPaint,
                )
                RunOrientation.SIDEWAYS_ROTATED -> {
                    // Each emitted point is absolute. Rotating one glyph at a
                    // time around that pen position keeps Latin progressing
                    // down its vertical column rather than marching sideways.
                    run.glyphIds.indices.forEach { glyphIndex ->
                        val positionIndex = glyphIndex * 2
                        canvas.save()
                        canvas.rotate(
                            90f,
                            run.positions[positionIndex],
                            run.positions[positionIndex + 1],
                        )
                        canvas.drawGlyphs(
                            run.glyphIds,
                            glyphIndex,
                            run.positions,
                            positionIndex,
                            1,
                            font,
                            glyphPaint,
                        )
                        canvas.restore()
                    }
                }
            }
        }
    }

    private fun drawDecorations(canvas: Canvas, decorations: List<Decoration>) {
        decorations.forEach { decoration ->
            glyphPaint.color = colorFor(decoration.colorRole)
            canvas.drawRect(
                decoration.rect.x.toFloat(),
                decoration.rect.y.toFloat(),
                (decoration.rect.x + decoration.rect.width).toFloat(),
                (decoration.rect.y + decoration.rect.height).toFloat(),
                glyphPaint,
            )
        }
    }

    private fun drawImages(canvas: Canvas, images: List<ImagePlacement>) {
        images.forEach { placement ->
            val rect = RectF(
                placement.rect.x.toFloat(),
                placement.rect.y.toFloat(),
                (placement.rect.x + placement.rect.width).toFloat(),
                (placement.rect.y + placement.rect.height).toFloat(),
            )
            val bitmap = imageLoader?.image(placement.href) { invalidate() }
            if (bitmap == null) {
                drawImagePlaceholder(canvas, rect)
            } else {
                canvas.drawBitmap(bitmap, null, aspectFit(bitmap, rect), imagePaint)
            }
        }
    }

    private fun drawImagePlaceholder(canvas: Canvas, rect: RectF) {
        val secondary = colorFor(ColorRole.SECONDARY)
        glyphPaint.style = Paint.Style.FILL
        glyphPaint.color = (secondary and 0x00FFFFFF) or 0x14000000
        canvas.drawRect(rect, glyphPaint)
        glyphPaint.style = Paint.Style.STROKE
        glyphPaint.strokeWidth = 1f
        glyphPaint.color = (secondary and 0x00FFFFFF) or 0x33000000
        canvas.drawRect(rect.left + 0.5f, rect.top + 0.5f, rect.right - 0.5f, rect.bottom - 0.5f, glyphPaint)
        glyphPaint.style = Paint.Style.FILL
    }

    private fun aspectFit(bitmap: Bitmap, rect: RectF): RectF {
        if (bitmap.width <= 0 || bitmap.height <= 0) return rect
        val scale = min(rect.width() / bitmap.width, rect.height() / bitmap.height)
        val width = bitmap.width * scale
        val height = bitmap.height * scale
        return RectF(
            rect.centerX() - width / 2f,
            rect.centerY() - height / 2f,
            rect.centerX() + width / 2f,
            rect.centerY() + height / 2f,
        )
    }

    private fun colorFor(role: ColorRole): Int = when (role) {
        ColorRole.TEXT -> palette.text
        ColorRole.SECONDARY -> palette.secondary
        ColorRole.LINK -> palette.link
    }

    private fun renderedRun(run: app.inkuna.core.GlyphRun): RenderedGlyphRun? {
        val glyphIds = run.glyphIds.map { it.toInt() }.toIntArray()
        val positions = run.positions.toFloatArray()
        if (glyphIds.isEmpty() || positions.size != glyphIds.size * 2) return null
        return RenderedGlyphRun(
            fontId = run.fontId,
            size = run.size.toFloat(),
            colorRole = run.colorRole,
            glyphIds = glyphIds,
            positions = positions,
            orientation = run.orientation,
        )
    }

    private data class RenderedGlyphRun(
        val fontId: UInt,
        val size: Float,
        val colorRole: ColorRole,
        val glyphIds: IntArray,
        val positions: FloatArray,
        val orientation: RunOrientation,
    )
}
