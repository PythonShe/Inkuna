package app.inkuna.android.ui.reader

import org.readium.r2.navigator.epub.EpubNavigatorFragment
import org.readium.r2.navigator.input.InputListener
import org.readium.r2.navigator.input.Key
import org.readium.r2.navigator.input.KeyEvent
import org.readium.r2.navigator.input.TapEvent
import org.readium.r2.shared.ExperimentalReadiumApi

/**
 * Edge taps and hardware keys, routed through [ReaderPagerLayout] so a
 * tapped turn runs the same springs as a dragged one — including sliding
 * a neighbouring chapter in instead of snapping. Replaces Readium's
 * `DirectionalNavigationAdapter` with the same zones (30% of the width,
 * at least 80 dp — shared with the iOS shell): edge taps are geometric
 * like the drags, arrow keys are geometric, space reads on.
 */
@OptIn(ExperimentalReadiumApi::class)
class ReaderPageTurnListener(
    private val navigator: EpubNavigatorFragment,
    private val pager: () -> ReaderPagerLayout?,
) : InputListener {

    override fun onTap(event: TapEvent): Boolean {
        if (navigator.overflow.value.scroll) return false
        val target = pager() ?: return false
        val width = navigator.publicationView.width.toFloat()
        if (width <= 0f) return false
        val density = navigator.publicationView.resources.displayMetrics.density
        val band = maxOf(width * 0.3f, 80f * density)
        return when {
            event.point.x < band -> target.turnGeometric(-1)
            event.point.x > width - band -> target.turnGeometric(1)
            else -> false
        }
    }

    override fun onKey(event: KeyEvent): Boolean {
        if (event.type != KeyEvent.Type.Down) return false
        val target = pager() ?: return false
        return when (event.key) {
            Key.ArrowLeft -> target.turnGeometric(-1)
            Key.ArrowRight -> target.turnGeometric(1)
            Key.Space, Key.ArrowDown -> target.turnForward()
            Key.ArrowUp -> target.turnBackward()
            else -> false
        }
    }
}
