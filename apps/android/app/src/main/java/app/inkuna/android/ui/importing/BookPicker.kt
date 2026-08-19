package app.inkuna.android.ui.importing

import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState

/**
 * MIME types offered to the Storage Access Framework.
 *
 * `application/epub+zip` alone would grey out most real libraries: Downloads
 * and many cloud providers label an EPUB `application/octet-stream`, and
 * some label it `application/zip`. MOBI/AZW3 carry three MIME spellings in
 * the wild, and TXT is plain `text/plain`. Offering the wider set keeps
 * books selectable; anything the core cannot read is rejected with its
 * format named, which is a far better outcome than a file the reader can
 * see but not tap.
 */
val BookMimeTypes = arrayOf(
    "application/epub+zip",
    "application/x-epub+zip",
    "application/zip",
    "application/octet-stream",
    "application/x-mobipocket-ebook",
    "application/vnd.amazon.ebook",
    "application/vnd.amazon.mobi8-ebook",
    "text/plain",
)

/**
 * A one-shot launcher for the multi-select document picker.
 *
 * Cancelling the picker returns an empty list and does nothing — no cache
 * file is created before a document actually arrives, so a cancelled pick
 * cannot leak.
 */
@Composable
fun rememberBookPicker(onPicked: (List<Uri>) -> Unit): () -> Unit {
    val current by rememberUpdatedState(onPicked)
    val launcher = rememberLauncherForActivityResult(
        ActivityResultContracts.OpenMultipleDocuments()
    ) { uris ->
        if (uris.isNotEmpty()) current(uris)
    }
    return remember(launcher) { { launcher.launch(BookMimeTypes) } }
}
