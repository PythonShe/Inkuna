package app.inkuna.android.ui.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.CloudDownload
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.inkuna.android.R
import app.inkuna.android.ui.theme.InkTheme
import app.inkuna.android.ui.theme.InkType
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight

/** Library/search result row: cover, title/author, progress, cloud badge. */
@Composable
fun BookListRow(
    title: String,
    author: String,
    progress: Int?,
    seed: Int,
    coverPath: String?,
    downloaded: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val ink = InkTheme.colors
    Row(
        modifier = modifier
            .fillMaxWidth()
            .clickable(role = Role.Button, onClick = onClick)
            .padding(vertical = 12.dp),
        horizontalArrangement = Arrangement.spacedBy(14.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        BookCover(title = title, author = author, width = 52.dp, seed = seed, coverPath = coverPath)
        Column(Modifier.weight(1f)) {
            Text(
                title,
                style = InkType.heading.copy(fontSize = 17.sp, lineHeight = 22.sp),
                color = ink.textDisplay,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                author,
                style = InkType.label.copy(fontWeight = FontWeight.Normal),
                color = ink.textSecondary,
                modifier = Modifier.padding(top = 2.dp),
            )
            if (progress != null) {
                Row(
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier.padding(top = 8.dp),
                ) {
                    InkProgressBar(progress, Modifier.width(72.dp))
                    Text(
                        stringResource(R.string.reader_percent, progress),
                        style = InkType.caption,
                        color = ink.textTertiary,
                    )
                }
            }
        }
        if (!downloaded) {
            Icon(
                Icons.Outlined.CloudDownload,
                contentDescription = stringResource(R.string.a11y_not_downloaded),
                tint = ink.textTertiary,
                modifier = Modifier.size(20.dp),
            )
        }
    }
}
