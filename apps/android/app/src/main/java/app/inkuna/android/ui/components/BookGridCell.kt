package app.inkuna.android.ui.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.inkuna.android.R
import app.inkuna.android.ui.theme.InkTheme
import app.inkuna.android.ui.theme.InkType

/**
 * A library grid tile: the cover with its title beneath, nothing else — the
 * shelf treatment ([app.inkuna.android.ui.main.ShelfBookView]) minus the
 * author line, so a wall of covers stays quiet. The author still reaches
 * assistive tech through the row label, matching the list rows.
 */
@Composable
fun BookGridCell(
    title: String,
    author: String,
    width: Dp,
    seed: Int,
    coverPath: String?,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val rowLabel = stringResource(R.string.a11y_book_row, title, author)
    Column(
        modifier = modifier
            .width(width)
            .clickable(onClick = onClick)
            .clearAndSetSemantics {
                contentDescription = rowLabel
                role = Role.Button
            },
    ) {
        BookCover(
            title = title,
            author = author,
            width = width,
            seed = seed,
            coverPath = coverPath,
        )
        Text(
            title,
            style = InkType.heading.copy(fontSize = 14.sp, lineHeight = 18.sp),
            color = InkTheme.colors.textDisplay,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.padding(top = 9.dp),
        )
    }
}
