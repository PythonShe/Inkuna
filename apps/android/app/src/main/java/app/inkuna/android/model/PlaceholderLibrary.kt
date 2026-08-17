package app.inkuna.android.model

import app.inkuna.android.R

/**
 * The last design-prototype stand-ins, each waiting on a core capability
 * (and mirrored on iOS). Book *content* here is scenery and never
 * localizes; anything that reads as UI copy carries a string resource.
 */
data class PlaceholderBook(
    val title: String,
    val author: String,
    /** 0..100 */
    val progress: Int,
    val coverSeed: Int,
)

object PlaceholderLibrary {
    /** Tonight's hero card while the library holds nothing to continue. */
    val heroBook = PlaceholderBook("The Will of the Many", "James Islington", 40, 0)

    /**
     * The last stand-in left: collection chips on Tonight need collections
     * in the core. TODO(core): replace with real collection filters.
     */
    val tonightChips = listOf(
        R.string.tonight_chip_fiction,
        R.string.tonight_chip_essays,
        R.string.tonight_chip_night_reads,
    )
}
