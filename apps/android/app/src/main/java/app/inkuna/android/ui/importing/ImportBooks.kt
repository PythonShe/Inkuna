package app.inkuna.android.ui.importing

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.inkuna.android.R
import app.inkuna.android.importing.ImportEngine
import app.inkuna.android.importing.ImportState
import app.inkuna.android.ui.components.InkButton
import app.inkuna.android.ui.components.InkButtonSize
import app.inkuna.android.ui.theme.InkSpace
import app.inkuna.android.ui.theme.InkTheme
import app.inkuna.android.ui.theme.InkType

/**
 * The whole EPUB import experience, as one composable.
 *
 * Drop it anywhere a library surface wants an "add books" affordance; it
 * owns the document picker, the `content://` → cache bridge, the core call,
 * the progress sheet, and the report. Nothing else has to be wired.
 *
 * ```kotlin
 * ImportBooks(libraryIsEmpty = books.isEmpty(), onLibraryChanged = { reload() })
 * ```
 *
 * The `Bookshelf` comes from
 * [app.inkuna.android.model.LibraryStore] by way of
 * [ImportEngine] — this layer never opens one.
 *
 * @param libraryIsEmpty renders the first-import invitation instead of a
 *   plain button. The empty shelf is where an import matters most, so it
 *   gets the full block rather than a button hiding under a title.
 * @param onLibraryChanged called once per run that actually added a book.
 *   The core is the source of truth; reload the shelf from it.
 */
@Composable
fun ImportBooks(
    modifier: Modifier = Modifier,
    libraryIsEmpty: Boolean = false,
    onLibraryChanged: () -> Unit = {},
) {
    val context = LocalContext.current
    val state by ImportEngine.state.collectAsStateWithLifecycle()
    val pick = rememberEpubPicker { uris -> ImportEngine.start(context, uris) }

    val notify by rememberUpdatedState(onLibraryChanged)
    val report = (state as? ImportState.Finished)?.report
    // Keyed on the report instance: one run, one reload, even if the sheet
    // recomposes while the reader reads it.
    LaunchedEffect(report) {
        if (report?.changedLibrary == true) notify()
    }

    if (libraryIsEmpty) {
        EmptyLibraryInvite(onAdd = pick, modifier = modifier)
    } else {
        val label = stringResource(R.string.a11y_import_add_books)
        InkButton(
            text = stringResource(R.string.import_add_books),
            onClick = pick,
            modifier = modifier.semantics { contentDescription = label },
        )
    }

    ImportSheet(
        state = state,
        onCancel = ImportEngine::cancel,
        onDismiss = ImportEngine::dismiss,
    )
}

/**
 * The empty shelf: an invitation, not an error. Quiet serif line, one
 * sentence of what will happen, one button — the same restraint the rest of
 * the app's empty states use, with the difference that this one can be
 * acted on.
 */
@Composable
fun EmptyLibraryInvite(
    onAdd: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val ink = InkTheme.colors
    Column(
        modifier = modifier
            .fillMaxWidth()
            .padding(vertical = InkSpace.s12),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        Text(
            stringResource(R.string.import_empty_title),
            style = InkType.sectionTitle,
            color = ink.textDisplay,
            textAlign = TextAlign.Center,
        )
        Spacer(Modifier.height(InkSpace.s2))
        Text(
            stringResource(R.string.import_empty_body),
            style = InkType.reading,
            color = ink.textTertiary,
            textAlign = TextAlign.Center,
            modifier = Modifier.padding(horizontal = InkSpace.s4),
        )
        Spacer(Modifier.height(InkSpace.s6))
        InkButton(
            text = stringResource(R.string.import_empty_action),
            onClick = onAdd,
            size = InkButtonSize.Large,
        )
    }
}
