package app.inkuna.android.ui.library

import androidx.annotation.StringRes
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.itemsIndexed
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.List
import androidx.compose.material.icons.outlined.GridView
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.inkuna.android.R
import app.inkuna.android.model.AppSettings
import app.inkuna.android.ui.components.BookGridCell
import app.inkuna.android.ui.components.BookListRow
import app.inkuna.android.ui.components.InkSearchField
import app.inkuna.android.ui.components.InkSegmentedControl
import app.inkuna.android.ui.importing.AddBooksButton
import app.inkuna.android.ui.importing.EmptyLibraryInvite
import app.inkuna.android.ui.importing.ImportBooksHost
import app.inkuna.android.ui.main.DisplayTitle
import app.inkuna.android.ui.main.EmptyState
import app.inkuna.android.ui.theme.InkSpace
import app.inkuna.android.ui.theme.InkTheme
import kotlinx.coroutines.flow.filter

private val SEGMENT_LABELS: Map<LibrarySegment, Int> = mapOf(
    LibrarySegment.Reading to R.string.library_seg_reading,
    LibrarySegment.Finished to R.string.library_seg_finished,
    LibrarySegment.Wishlist to R.string.library_seg_wishlist,
)

@StringRes
private fun emptyMessage(emptiness: LibraryEmptiness): Int = when (emptiness) {
    is LibraryEmptiness.WholeLibrary -> R.string.library_empty_all
    is LibraryEmptiness.Shelf -> when (emptiness.kind) {
        LibraryEmptiness.Shelf.Kind.Finished -> R.string.library_empty_finished
        LibraryEmptiness.Shelf.Kind.Wishlist -> R.string.library_empty_wishlist
        LibraryEmptiness.Shelf.Kind.NoMatches -> R.string.library_empty_query
        LibraryEmptiness.Shelf.Kind.Unopenable -> R.string.library_unopenable
        LibraryEmptiness.Shelf.Kind.Reading -> R.string.library_empty_reading
    }
}

/** The cover width the grid aims for on phones (the shelf tile width);
 *  columns are however many fit, never fewer than two. Matches iOS. */
private val GRID_CELL_TARGET = 104.dp

/** The airier target on tablets (smallest screen width >= 600dp, the
 *  platform's own sw600dp convention): an 11" tablet lands on three
 *  columns in portrait and four in landscape. Matches iOS. */
private val GRID_CELL_TARGET_TABLET = 250.dp

/** Horizontal gap between grid cells. */
private val GRID_GAP = InkSpace.s3

/** Vertical gap between grid rows. */
private val GRID_ROW_GAP = InkSpace.s5

/**
 * The Library tab: search, the Reading/Finished/Wishlist segments, and the
 * books — as list rows or a cover grid, the reader's persisted choice. The
 * whole tab is one lazy grid: the header composes as full-span items, list
 * rows span every column, and search results always come back as list rows
 * (a match's title and author are the evidence it matched).
 *
 * TODO(core): the Wishlist segment has no core shelf yet (file-less
 * publications are deferred to their own spec), so it stays visible and
 * empty.
 */
@Composable
fun LibraryScreen(
    innerPadding: PaddingValues,
    onOpenBook: (String) -> Unit,
    model: LibraryViewModel = viewModel(),
) {
    val state by model.state.collectAsStateWithLifecycle()
    val context = LocalContext.current
    val settings = remember(context) { AppSettings.get(context) }
    val prefs by settings.snapshot.collectAsStateWithLifecycle()

    // Shelf membership moves behind this screen's back — a book marked
    // finished on its detail screen must have changed shelves by the time
    // the pop lands here (the Tonight tab reloads the same way).
    LifecycleResumeEffect(Unit) {
        model.reload()
        onPauseOrDispose {}
    }

    val segments = LibrarySegment.entries
    // Addressed by index: two shelves may well share a translation, and a
    // label round-trip would then pick the wrong one.
    val segmentLabels = segments.map { stringResource(SEGMENT_LABELS.getValue(it)) }

    // Search results always render as rows: a match's title and author are
    // the evidence it matched, and covers alone can't carry that.
    val grid = prefs.libraryGrid && state.query.isBlank()

    // ScrollScreen's manners, replicated for the lazy container: the IME is
    // an inset under edge-to-edge, and scrolling — or a tap on quiet
    // space — puts the keyboard away, as the search screens on iOS do.
    val gridState = rememberLazyGridState()
    val focusManager = LocalFocusManager.current
    LaunchedEffect(gridState) {
        snapshotFlow { gridState.isScrollInProgress }
            .filter { it }
            .collect { focusManager.clearFocus() }
    }

    ImportBooksHost(onLibraryChanged = model::reload) { onAdd ->
        BoxWithConstraints(
            modifier = Modifier
                .fillMaxSize()
                .padding(innerPadding)
                .imePadding()
                .pointerInput(Unit) { detectTapGestures { focusManager.clearFocus() } },
        ) {
            // Columns adapt to whatever width the window offers — rotation,
            // split screen, tablets — from the target tile width (phone or
            // tablet), floored at two so covers never balloon on narrow
            // phones.
            val cellTarget = if (LocalConfiguration.current.smallestScreenWidthDp >= 600) {
                GRID_CELL_TARGET_TABLET
            } else {
                GRID_CELL_TARGET
            }
            val available = maxWidth - InkSpace.pageMargin * 2
            val columns = maxOf(2, ((available + GRID_GAP) / (cellTarget + GRID_GAP)).toInt())
            val cellWidth = (available - GRID_GAP * (columns - 1)) / columns

            LazyVerticalGrid(
                columns = GridCells.Fixed(columns),
                state = gridState,
                horizontalArrangement = Arrangement.spacedBy(GRID_GAP),
                contentPadding = PaddingValues(
                    start = InkSpace.pageMargin,
                    end = InkSpace.pageMargin,
                    top = InkSpace.s6,
                    bottom = InkSpace.s8,
                ),
                modifier = Modifier.fillMaxSize(),
            ) {
                // One header item, so toggling the mode never recomposes the
                // search field out from under the keyboard.
                item(key = "header", span = { GridItemSpan(maxLineSpan) }) {
                    Column {
                        // The "+" rides beside the title, as it does on iOS —
                        // one affordance in one place across both shells.
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Box(Modifier.weight(1f)) {
                                DisplayTitle(stringResource(R.string.library_title))
                            }
                            ViewModeButton(
                                grid = prefs.libraryGrid,
                                onToggle = { settings.setLibraryGrid(!prefs.libraryGrid) },
                            )
                            AddBooksButton(onAdd = onAdd)
                        }
                        Spacer(Modifier.height(InkSpace.s5))
                        InkSearchField(
                            value = state.query,
                            onValueChange = model::setQuery,
                            placeholder = stringResource(R.string.library_search_placeholder),
                        )
                        Spacer(Modifier.height(InkSpace.s4))
                        InkSegmentedControl(
                            options = segmentLabels,
                            selectedIndex = segments.indexOf(state.segment),
                            onSelect = { index -> model.setSegment(segments[index]) },
                            modifier = Modifier.fillMaxWidth(),
                        )
                        Spacer(Modifier.height(InkSpace.s2))
                    }
                }
                if (state.rows.isEmpty()) {
                    // Empty states stand full-width — an empty library gets
                    // the invitation to import, the one place a full block
                    // earns its space over the "+" above.
                    item(key = "empty", span = { GridItemSpan(maxLineSpan) }) {
                        if (state.emptiness is LibraryEmptiness.WholeLibrary) {
                            EmptyLibraryInvite(onAdd = onAdd)
                        } else {
                            EmptyState(stringResource(emptyMessage(state.emptiness)))
                        }
                    }
                } else if (grid) {
                    itemsIndexed(state.rows, key = { _, row -> row.id }) { index, row ->
                        BookGridCell(
                            title = row.title,
                            author = row.author,
                            width = cellWidth,
                            seed = row.seed,
                            coverPath = row.coverPath,
                            onClick = { onOpenBook(row.id) },
                            // Row gap by hand: spacedBy would also space the
                            // header items, which keep their own rhythm.
                            modifier = Modifier.padding(
                                top = if (index >= columns) GRID_ROW_GAP else 0.dp,
                            ),
                        )
                    }
                } else {
                    itemsIndexed(state.rows, key = { _, row -> row.id }, span = { _, _ -> GridItemSpan(maxLineSpan) }) { _, row ->
                        BookListRow(
                            title = row.title,
                            author = row.author,
                            progress = row.progress,
                            seed = row.seed,
                            coverPath = row.coverPath,
                            // The core owns every book's file, so a listed book
                            // is always on disk — no cloud-only state to badge.
                            downloaded = true,
                            onClick = { onOpenBook(row.id) },
                        )
                    }
                }
            }
        }
    }
}

/**
 * The list/grid toggle beside the title. The glyph shows what a tap switches
 * to — a grid while rows are showing, rows while the grid is up.
 */
@Composable
private fun ViewModeButton(grid: Boolean, onToggle: () -> Unit, modifier: Modifier = Modifier) {
    IconButton(onClick = onToggle, modifier = modifier) {
        Icon(
            if (grid) Icons.AutoMirrored.Outlined.List else Icons.Outlined.GridView,
            contentDescription = stringResource(
                if (grid) R.string.library_view_list else R.string.library_view_grid,
            ),
            tint = InkTheme.colors.textDisplay,
        )
    }
}
