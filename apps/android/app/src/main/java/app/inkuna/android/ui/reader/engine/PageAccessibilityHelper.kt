package app.inkuna.android.ui.reader.engine

import android.graphics.Rect
import android.os.Bundle
import android.text.SpannableString
import android.text.Spanned
import android.text.style.LocaleSpan
import androidx.core.view.accessibility.AccessibilityNodeInfoCompat
import androidx.customview.widget.ExploreByTouchHelper
import app.inkuna.core.A11yRole
import java.util.IllformedLocaleException
import java.util.Locale
import kotlin.math.roundToInt

/** Exposes the already-present page display list's logical a11y blocks. */
class PageAccessibilityHelper(
    private val pageView: PageView,
) : ExploreByTouchHelper(pageView) {
    fun onPageContentChanged() {
        invalidateRoot()
    }

    override fun getVirtualViewAt(x: Float, y: Float): Int {
        val density = pageView.resources.displayMetrics.density
        val pageX = x / density
        val pageY = y / density
        return pageView.accessibilityBlocks().indexOfFirst { block ->
            pageX >= block.rect.x && pageX <= block.rect.x + block.rect.width &&
                pageY >= block.rect.y && pageY <= block.rect.y + block.rect.height
        }.takeIf { it >= 0 } ?: INVALID_ID
    }

    override fun getVisibleVirtualViews(virtualViewIds: MutableList<Int>) {
        pageView.accessibilityBlocks().indices.forEach(virtualViewIds::add)
    }

    override fun onPopulateNodeForVirtualView(
        virtualViewId: Int,
        node: AccessibilityNodeInfoCompat,
    ) {
        val block = pageView.accessibilityBlocks().getOrNull(virtualViewId)
        if (block == null) {
            node.contentDescription = ""
            node.setBoundsInParent(Rect())
            node.isVisibleToUser = false
            return
        }
        node.text = localizedText(block.text, block.lang)
        node.setBoundsInParent(bounds(block))
        node.isVisibleToUser = true
        node.isHeading = block.role == A11yRole.HEADING
        if (block.isLink) {
            node.isClickable = true
            node.addAction(AccessibilityNodeInfoCompat.AccessibilityActionCompat.ACTION_CLICK)
        }
    }

    override fun onPerformActionForVirtualView(
        virtualViewId: Int,
        action: Int,
        arguments: Bundle?,
    ): Boolean {
        if (action != AccessibilityNodeInfoCompat.ACTION_CLICK) return false
        val block = pageView.accessibilityBlocks().getOrNull(virtualViewId) ?: return false
        return pageView.activateLink(block)
    }

    private fun bounds(block: app.inkuna.core.A11yBlock): Rect {
        val density = pageView.resources.displayMetrics.density
        return Rect(
            (block.rect.x * density).roundToInt(),
            (block.rect.y * density).roundToInt(),
            ((block.rect.x + block.rect.width) * density).roundToInt(),
            ((block.rect.y + block.rect.height) * density).roundToInt(),
        )
    }

    private fun localizedText(text: String, languageTag: String?): CharSequence {
        val locale = languageTag?.takeIf(String::isNotBlank)?.let(::localeForTag) ?: return text
        return SpannableString(text).apply {
            setSpan(LocaleSpan(locale), 0, length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
        }
    }

    private fun localeForTag(tag: String): Locale? = try {
        Locale.Builder().setLanguageTag(tag).build().takeIf { it.language.isNotEmpty() }
    } catch (_: IllformedLocaleException) {
        null
    }
}
