package app.inkuna.android.ui.reader

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.focusTarget
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEvent
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onKeyEvent
import androidx.compose.ui.input.key.type

/**
 * Hardware page turns for keyboard cases, DeX and ChromeOS, routed through
 * [ReaderPagerLayout] so a keyed turn runs the same springs as a dragged one.
 * The mapping mirrors the iOS key commands
 * (`ReaderViewController+Chrome.swift`) and the pre-engine Android listener:
 * the arrows are geometric like the edge taps and the drags, space and the
 * down arrow read on, the up arrow steps back.
 *
 * The reader itself is the focus target, so the keys reach it whether or not
 * a chrome control holds focus; because [onKeyEvent] bubbles up from the
 * focused node, a focused text field still consumes its own typing first.
 *
 * [suppressed] covers the surfaces that own the keyboard while they are up —
 * the modal sheets and the search panel. A live text selection suppresses
 * them too: while a selection is up the keys belong to it, not to the pager.
 */
@Composable
internal fun Modifier.readerKeyTurns(host: EngineHost?, suppressed: Boolean): Modifier {
    val focus = remember { FocusRequester() }
    LaunchedEffect(host, suppressed) {
        if (host != null && !suppressed) runCatching { focus.requestFocus() }
    }
    return onKeyEvent { event ->
        val live = host ?: return@onKeyEvent false
        if (suppressed) false else handleReaderKey(event, live)
    }
        .focusRequester(focus)
        .focusTarget()
}

/**
 * Returns whether the key was a page turn; a turn key is consumed even when
 * the pager declines it, so a declined turn never leaks into focus traversal.
 */
private fun handleReaderKey(event: KeyEvent, host: EngineHost): Boolean {
    if (event.type != KeyEventType.KeyDown) return false
    if (host.selection.isActive) return false
    return when (event.key) {
        Key.DirectionLeft -> { host.layout.turnGeometric(-1); true }
        Key.DirectionRight -> { host.layout.turnGeometric(1); true }
        Key.Spacebar, Key.DirectionDown -> { host.layout.turnForward(); true }
        Key.DirectionUp -> { host.layout.turnBackward(); true }
        else -> false
    }
}
