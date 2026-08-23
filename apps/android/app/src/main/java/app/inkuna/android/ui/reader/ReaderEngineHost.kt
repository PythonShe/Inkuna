package app.inkuna.android.ui.reader

import android.content.Intent
import android.view.View
import androidx.compose.runtime.MutableState
import androidx.core.net.toUri
import app.inkuna.android.ui.reader.engine.EnginePageCanvas
import app.inkuna.android.ui.reader.engine.EnginePagerSurface
import app.inkuna.android.ui.reader.engine.ReaderSelectionController
import app.inkuna.core.Coordinate
import kotlinx.coroutines.suspendCancellableCoroutine

/** The engine-backed view stack one [ReaderScreen] composition drives. */
internal class EngineHost(
    val layout: ReaderPagerLayout,
    val canvas: EnginePageCanvas,
    val surface: EnginePagerSurface,
    val selection: ReaderSelectionController,
)

/**
 * One parked navigation, retried on layout events for its own spine only.
 * [toChapterEnd] marks a deferred backward chapter turn, which must wait
 * for complete geometry (the last page) rather than the published prefix.
 */
internal data class PendingJump(
    val coordinate: Coordinate,
    val toChapterEnd: Boolean = false,
    val linkToast: Boolean = false,
    val showChrome: Boolean = true,
)

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
 */
internal fun handleCanvasPoint(
    book: ReaderViewModel.ReaderBook,
    host: EngineHost,
    x: Float,
    y: Float,
    onLinkFailed: () -> Unit,
    onInternalLink: (Coordinate) -> Unit,
    chromeVisible: MutableState<Boolean>,
    menuOpen: MutableState<Boolean>,
) {
    val hit = runCatching {
        book.session.hitTest(host.surface.spineIdx, host.surface.pageIdx, host.canvas.toLayoutX(x), host.canvas.toLayoutY(y))
    }.getOrNull()
    val target = hit?.linkTarget
    if (target != null) {
        val uri = target.toUri()
        if (uri.scheme == "http" || uri.scheme == "https") {
            runCatching { host.canvas.context.startActivity(Intent(Intent.ACTION_VIEW, uri)) }.onFailure { onLinkFailed() }
        } else {
            runCatching { book.session.locateHrefParts(target) }
                .onSuccess(onInternalLink)
                .onFailure { onLinkFailed() }
        }
        return
    }
    val band = maxOf(host.layout.width * 0.3f, 80f)
    when {
        x < band -> host.layout.turnBackward()
        x > host.layout.width - band -> host.layout.turnForward()
        else -> {
            menuOpen.value = false
            chromeVisible.value = !chromeVisible.value
        }
    }
}
