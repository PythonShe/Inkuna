package app.inkuna.android.model

import androidx.annotation.StringRes
import app.inkuna.android.R

/**
 * Placeholder shelf mirroring the design prototype (and the iOS shell).
 * TODO(core): replace with `app.inkuna.core.Bookshelf` queries once the
 * reading-progress and metadata stores land. Book *content* (titles,
 * authors, prose, chapter names) is stand-in and never localizes; anything
 * that reads as UI copy carries a string resource instead.
 */
data class PlaceholderBook(
    val id: Int,
    val title: String,
    val author: String,
    /** 0..100 */
    val progress: Int,
    val currentPage: Int,
    val pageCount: Int,
    val coverSeed: Int,
    val downloaded: Boolean,
)

/** A stat tile: a stand-in numeral with a localized caption. */
data class PlaceholderFact(val value: String, @param:StringRes val captionRes: Int)

data class PlaceholderChapter(val numeral: String, val title: String, val page: Int)

object PlaceholderLibrary {
    val books = listOf(
        PlaceholderBook(0, "The Will of the Many", "James Islington", 40, 564, 1418, 0, downloaded = true),
        PlaceholderBook(1, "Letters at Dusk", "I. Sato", 64, 198, 310, 2, downloaded = true),
        PlaceholderBook(2, "Jade City", "Fonda Lee", 17, 87, 512, 3, downloaded = true),
        PlaceholderBook(3, "West with the Night", "Beryl Markham", 33, 96, 294, 1, downloaded = false),
        PlaceholderBook(4, "The Quiet Codex", "A. Moreno", 5, 21, 402, 4, downloaded = false),
        PlaceholderBook(5, "A Winter Ledger", "H. Brandt", 1, 4, 356, 5, downloaded = true),
    )

    val heroBook = books[0]

    /** "On the nightstand" shelf on Tonight. */
    val shelf = books.subList(1, 5)

    /** "New this week" on the empty Search screen. */
    val discover = listOf(books[3], books[4], books[5])

    val chapters = listOf(
        PlaceholderChapter("I", "The Sea Wall", 1),
        PlaceholderChapter("II", "Letters from the Capital", 38),
        PlaceholderChapter("III", "The Long Gallery", 84),
        PlaceholderChapter("IV", "A Debt of Ink", 141),
        PlaceholderChapter("V", "The Night Ferry", 210),
        PlaceholderChapter("VI", "What the Archivist Kept", 296),
        PlaceholderChapter("VII", "The Lamp and the Moon", 541),
        PlaceholderChapter("VIII", "The Turning Tide", 640),
        PlaceholderChapter("IX", "The Quiet Hours", 733),
        PlaceholderChapter("X", "Ashes and Vellum", 829),
    )

    const val currentChapterIndex = 6

    const val chapterEyebrow = "VII · The lamp and the moon"

    /** Four sample pages, three paragraphs each. */
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

    // Stats — hardcoded prototype data. TODO(core): derive from recorded
    // reading sessions. Month and weekday names are formatted from
    // java.time in the caller's locale, never spelled out here.
    val facts = listOf(
        PlaceholderFact("214", R.string.stats_pages_this_week),
        PlaceholderFact("6½", R.string.stats_hours_this_month),
        PlaceholderFact("12", R.string.stats_books_this_year),
    )
    val calendarMonth: java.time.Month = java.time.Month.AUGUST
    const val calendarLeadingBlanks = 6
    const val calendarDayCount = 31
    const val calendarToday = 14
    val calendarReadDays = setOf(1, 2, 3, 5, 8, 9, 10, 11, 13, 14)

    val tonightChips = listOf(
        R.string.tonight_chip_fiction,
        R.string.tonight_chip_essays,
        R.string.tonight_chip_night_reads,
    )
}
