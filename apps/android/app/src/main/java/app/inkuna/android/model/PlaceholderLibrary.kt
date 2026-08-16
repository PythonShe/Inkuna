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

    /** TODO(core): collection chips on Tonight need collections in the core. */
    val tonightChips = listOf(
        R.string.tonight_chip_fiction,
        R.string.tonight_chip_essays,
        R.string.tonight_chip_night_reads,
    )

    /**
     * Four sample pages, three paragraphs each — the fake in-book search
     * corpus. TODO(core): replace with core-backed in-book search.
     */
    val samplePages: List<List<String>> = listOf(
        listOf(
            "The lamp burned low, and the moon took over the work of lighting the page. Outside, the city had gone quiet in the particular way it does after midnight — not silent, but hushed, as though it too were reading over her shoulder.",
            "She turned the page with a thumb worn soft by ten thousand such turnings. The paper made its small dry sound, the sound of a door closing gently in another room.",
            "“Stay,” the chapter seemed to say. And she stayed — one more page, then one more, the old bargain readers make with the night and always, gladly, lose.",
        ),
        listOf(
            "By the window the tea had gone cold an hour ago. It didn’t matter. Some rituals are about the object; this one was about the light, the ink, and the staying.",
            "The book had come to her third-hand, its spine already broken in at the good chapters, like a trail worn by earlier travellers. She liked that. A book that has been loved arrives already warm.",
            "Somewhere below, a late tram sighed along its rails. She read the same sentence twice, not because it was difficult but because it deserved it.",
        ),
        listOf(
            "There is an hour — readers know it — when the house finishes settling and the margins seem to widen, when even the clock lowers its voice.",
            "In that hour the story stopped being words. The sea wall was under her hands; the archivist’s lamp was her lamp; the letters from the capital were addressed, plainly, to her.",
            "She reached for the cold tea anyway, out of loyalty.",
        ),
        listOf(
            "One more page, she told the night. The night, which has heard this promise from every reader since ink was first set to paper, said nothing and let her keep it badly.",
            "The chapter closed the way good chapters do — not with a door slammed, but with a lamp carried into the next room, its light still visible under the sill.",
            "Swipe to keep going, or rest here.",
        ),
    )
}
