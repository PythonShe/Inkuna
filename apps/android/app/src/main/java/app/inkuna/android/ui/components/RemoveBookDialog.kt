package app.inkuna.android.ui.components

import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import app.inkuna.android.R
import app.inkuna.android.ui.theme.InkTheme
import app.inkuna.android.ui.theme.InkType

/**
 * The one gate in front of an irreversible delete: it names the book so a
 * mis-tap on the wrong shelf row is caught here rather than regretted.
 */
@Composable
fun RemoveBookDialog(
    title: String,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    val ink = InkTheme.colors
    AlertDialog(
        onDismissRequest = onDismiss,
        containerColor = ink.bgSurface,
        titleContentColor = ink.textDisplay,
        title = {
            Text(stringResource(R.string.remove_confirm_title, title), style = InkType.heading)
        },
        text = {
            Text(
                stringResource(R.string.remove_confirm_body),
                style = InkType.ui,
                color = ink.textSecondary,
            )
        },
        confirmButton = {
            TextButton(onClick = onConfirm) {
                Text(stringResource(R.string.remove_confirm_button), color = ink.danger)
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) {
                Text(stringResource(R.string.remove_cancel), color = ink.textSecondary)
            }
        },
    )
}
