package app.inkuna.android.model

import app.inkuna.core.Publication
import kotlin.math.roundToInt

/**
 * One book row, projected off the core's [Publication] for the list and
 * shelf screens.
 *
 * Deliberately not the generated type: UniFFI records expose `var` fields,
 * which Compose treats as unstable and recomposes on every read.
 */
data class BookRow(
    val id: String,
    val title: String,
    val author: String,
    /** Whole percent read, or null for a book not yet begun. */
    val progress: Int?,
    val seed: Int,
    /** Absolute path of the core-extracted cover art, if the book has any. */
    val coverPath: String?,
) {
    companion object {
        /** [unknownAuthor] is the localized stand-in for a book naming none. */
        fun from(publication: Publication, unknownAuthor: String) = BookRow(
            id = publication.id,
            title = publication.title,
            author = publication.authors.joinToString(", ").ifEmpty { unknownAuthor },
            // Rounded, not truncated — iOS rounds, and the same book must
            // not read 1% on one shell and 2% on the other.
            progress = publication.progression.takeIf { it > 0.0 }
                ?.let { (it * 100).roundToInt() },
            seed = coverSeed(publication.id),
            coverPath = publication.coverPath,
        )
    }
}
