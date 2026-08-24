package app.inkuna.android.ui.reader

import android.content.Intent
import android.view.View
import androidx.compose.runtime.MutableState
import androidx.core.net.toUri
import app.inkuna.android.ui.reader.engine.EnginePageCanvas
import app.inkuna.android.ui.reader.engine.EnginePagerSurface
import app.inkuna.android.ui.reader.engine.ReaderSelectionController
import app.inkuna.core.Coordinate
import app.inkuna.core.InkunaException
import app.inkuna.core.ReaderSession
import kotlinx.coroutines.suspendCancellableCoroutine

/** The engine-backed view stack one [ReaderScreen] composition drives. */
internal class EngineHost(
    val layout: ReaderPagerLayout,
    val canvas: EnginePageCanvas,
    val surface: EnginePagerSurface,
    val selection: ReaderSelectionController,
)

/**
 * A fragment whose chapter has not laid out yet. The anchor map is built
 * during layout, so until the chapter completes `locateHref` answers
 * `NotReady` — which is not the anchor being absent. The jump carries the
 * fragment so the chapter's readiness event can resolve it.
 */
internal data class PendingAnchor(val href: String, val fragment: String)

/**
 * One parked navigation, retried on layout events for its own spine only.
 * [toChapterEnd] marks a deferred backward chapter turn, which must wait
 * for complete geometry (the last page) rather than the published prefix.
 */
internal data class PendingJump(
    val coordinate: Coordinate,
    val anchor: PendingAnchor? = null,
    val matchLength: ULong? = null,
    val toChapterEnd: Boolean = false,
    val linkToast: Boolean = false,
    val showChrome: Boolean = true,
)

/**
 * Turns a TOC or link href into a jump. A fragment needs the target
 * chapter's anchor map, which layout builds — so an un-laid chapter answers
 * `NotReady`, not `AnchorNotFound`. The fragment-free lookup resolves from
 * the spine model alone and never waits, so the jump still names the
 * chapter, aims at its start, and carries the fragment for that chapter's
 * readiness event to refine. Mirrors the iOS `resolveJump`.
 */
internal fun ReaderSession.resolveJump(href: String, linkToast: Boolean = false): PendingJump {
    val index = href.indexOf('#')
    if (index < 0) return PendingJump(locateHref(href, null), linkToast = linkToast)
    val path = href.substring(0, index)
    val fragment = href.substring(index + 1)
    val chapterStart = locateHref(path, null)
    return try {
        PendingJump(locateHref(path, fragment), linkToast = linkToast)
    } catch (_: InkunaException.NotReady) {
        PendingJump(chapterStart, anchor = PendingAnchor(path, fragment), linkToast = linkToast)
    }
}

/** Suspends until the view has a non-zero laid-out size. */
internal suspend fun View.awaitSized() {
    if (width > 0 && height > 0) return
    suspendCancellableCoroutine { continuation ->
        val listener = object : View.OnLayoutChangeListener {
            override fun onLayoutChange(
                view: View, left: Int, top: Int, right: Int, bottom: Int,
                oldLeft: Int, oldTop: Int, oldRight: Int, oldBottom: Int,
            ) {
                if (right - left > 0 && bottom - top > 0) {
                    view.removeOnLayoutChangeListener(this)
                    continuation.resume(Unit) { _, _, _ -> }
                }
            }
        }
        addOnLayoutChangeListener(listener)
        continuation.invokeOnCancellation { removeOnLayoutChangeListener(listener) }
    }
}

/**
 * Resolves a canvas tap: an external link opens in the browser, an internal
 * link jumps, and a plain point falls through to the edge-band page turns or
 * the chrome toggle.
 *
 * [x] and [y] are canvas pixels — the sole pixel-to-layout-point conversion
 * for this path happens here.
 */
internal fun handleCanvasPoint(
    book: ReaderViewModel.ReaderBook,
    host: EngineHost,
    x: Float,
    y: Float,
    onLinkFailed: () -> Unit,
    onInternalLink: (PendingJump) -> Unit,
    chromeVisible: MutableState<Boolean>,
    menuOpen: MutableState<Boolean>,
) {
    val target = linkTargetAt(
        book, host.surface.spineIdx, host.surface.pageIdx,
        host.canvas.toLayoutX(x), host.canvas.toLayoutY(y),
    )
    if (target != null) {
        followLink(book, host, target, onLinkFailed, onInternalLink)
        return
    }
    // Edge taps are geometric, like the drags and like the iOS shell's
    // bands: the left band always brings the page in from the left, whatever
    // the publication's progression direction.
    val band = maxOf(host.layout.width * 0.3f, 80f)
    when {
        x < band -> host.layout.turnGeometric(-1)
        x > host.layout.width - band -> host.layout.turnGeometric(1)
        else -> {
            menuOpen.value = false
            chromeVisible.value = !chromeVisible.value
        }
    }
}

/**
 * Accessibility activation of a link block. Its point arrives in the core's
 * own page-local space — layout points at 1x with y growing downward,
 * straight off `A11yBlock.rect` — which is exactly the space `hitTest` takes,
 * so it needs no view-space conversion (unlike [handleCanvasPoint], whose
 * point starts out canvas pixels). A miss reports the failure rather than
 * falling through to the edge bands: activating a link never turns the page.
 * Mirrors the iOS `activateLink`.
 */
internal fun handleLinkActivation(
    book: ReaderViewModel.ReaderBook,
    host: EngineHost,
    spineIdx: UInt,
    pageIdx: UInt,
    layoutX: Float,
    layoutY: Float,
    onLinkFailed: () -> Unit,
    onInternalLink: (PendingJump) -> Unit,
) {
    val target = linkTargetAt(book, spineIdx, pageIdx, layoutX.toDouble(), layoutY.toDouble())
    if (target == null) {
        onLinkFailed()
        return
    }
    followLink(book, host, target, onLinkFailed, onInternalLink)
}

/** Hit-tests one published page; [layoutX] and [layoutY] are layout points. */
private fun linkTargetAt(
    book: ReaderViewModel.ReaderBook,
    spineIdx: UInt,
    pageIdx: UInt,
    layoutX: Double,
    layoutY: Double,
): String? = runCatching {
    book.session.hitTest(spineIdx, pageIdx, layoutX, layoutY)
}.getOrNull()?.linkTarget

private fun followLink(
    book: ReaderViewModel.ReaderBook,
    host: EngineHost,
    target: String,
    onLinkFailed: () -> Unit,
    onInternalLink: (PendingJump) -> Unit,
) {
    val uri = target.toUri()
    if (uri.scheme == "http" || uri.scheme == "https") {
        runCatching { host.canvas.context.startActivity(Intent(Intent.ACTION_VIEW, uri)) }.onFailure { onLinkFailed() }
    } else {
        runCatching { book.session.resolveJump(target, linkToast = true) }
            .onSuccess(onInternalLink)
            .onFailure { onLinkFailed() }
    }
}
