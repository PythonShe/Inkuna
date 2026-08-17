package app.inkuna.android.ui.reader

import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.max

/**
 * The reader's vertical reading band, mirrored verbatim by the iOS shell
 * (`apps/ios/Inkuna/Reader/ReaderMetrics.swift`): both platforms must
 * render the same air around the page on comparable hardware. Change a
 * value here, change its sibling too.
 *
 * Phones pad relative to the system insets — the caller hands in the
 * larger of the status-bar and display-cutout insets, so the bonus is
 * pure breathing room — with floors so inset-less devices never feel
 * cramped. Tablets have no cutout and a broad canvas: a fixed, generous
 * band, not an inset-driven one.
 */
object ReaderMetrics {
    /** Air between the top system inset and the first line on phones. */
    val phoneTopBonus = 12.dp

    /** Floor for the top band on phones with small top insets. */
    val phoneTopMinimum = 40.dp

    /** Air between the bottom system inset and the last line on phones —
     *  wide enough that the page-info footer lives inside the band. */
    val phoneBottomBonus = 28.dp

    /** Floor for the bottom band on phones with button navigation. */
    val phoneBottomMinimum = 48.dp

    /** Fixed top and bottom band on tablets. */
    val tabletVertical = 64.dp

    /** The footer's bottom edge sits this far above the bottom inset. */
    val footerLift = 6.dp

    fun contentTop(systemTop: Dp, isTablet: Boolean): Dp =
        if (isTablet) tabletVertical else max(systemTop + phoneTopBonus, phoneTopMinimum)

    fun contentBottom(systemBottom: Dp, isTablet: Boolean): Dp =
        if (isTablet) tabletVertical else max(systemBottom + phoneBottomBonus, phoneBottomMinimum)
}
