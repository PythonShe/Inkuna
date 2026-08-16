import Foundation

// The last placeholder content standing, each entry waiting on a core
// capability that does not exist yet: the sample pages on the in-book
// search spec, the tonight chips on collections, the hero stand-in on
// nothing — it renders only in an empty library and is inert by design.

struct PlaceholderBook: Hashable {
    let title: String
    let author: String
    /// 0…1 fraction read.
    let progress: Double
    let coverSeed: Int
}

enum PlaceholderLibrary {
    /// The inert stand-in on the Tonight hero card while the library holds
    /// nothing to continue.
    static let heroBook = PlaceholderBook(
        title: "The Will of the Many",
        author: "James Islington",
        progress: 0.40,
        coverSeed: 0
    )

    // MARK: Reader sample (until the core's in-book search lands)

    static let pagesLeftText = "Fifteen pages left in this chapter"

    /// Four pages of sample prose, one array of paragraphs per page.
    static let samplePages: [[String]] = [
        [
            "The lamp burned low, and the moon took over the work of lighting the page. Outside, the city had gone quiet in the particular way it does after midnight — not silent, but hushed, as though it too were reading over her shoulder.",
            "She turned the page with a thumb worn soft by ten thousand such turnings. The paper made its small dry sound, the sound of a door closing gently in another room.",
            "“Stay,” the chapter seemed to say. And she stayed — one more page, then one more, the old bargain readers make with the night and always, gladly, lose.",
        ],
        [
            "By the window the tea had gone cold an hour ago. It didn’t matter. Some rituals are about the object; this one was about the light, the ink, and the staying.",
            "The book had come to her third-hand, its spine already broken in at the good chapters, like a trail worn by earlier travellers. She liked that. A book that has been loved arrives already warm.",
            "Somewhere below, a late tram sighed along its rails. She read the same sentence twice, not because it was difficult but because it deserved it.",
        ],
        [
            "There is an hour — readers know it — when the house finishes settling and the margins seem to widen, when even the clock lowers its voice.",
            "In that hour the story stopped being words. The sea wall was under her hands; the archivist’s lamp was her lamp; the letters from the capital were addressed, plainly, to her.",
            "She reached for the cold tea anyway, out of loyalty.",
        ],
        [
            "One more page, she told the night. The night, which has heard this promise from every reader since ink was first set to paper, said nothing and let her keep it badly.",
            "The chapter closed the way good chapters do — not with a door slammed, but with a lamp carried into the next room, its light still visible under the sill.",
            "Swipe to keep going, or rest here.",
        ],
    ]

    // MARK: Stats screen

    static let facts: [(value: String, caption: String)] = [
        ("214", "pages this week"),
        ("6½", "hours this month"),
        ("12", "books this year"),
    ]

    static let calendarMonthTitle = "August"
    /// Weekday column the 1st falls on (0 = Sunday).
    static let calendarLeadingBlanks = 6
    static let calendarDayCount = 31
    static let calendarToday = 14
    static let calendarReadDays: Set<Int> = [1, 2, 3, 5, 8, 9, 10, 11, 13, 14]
    static let calendarCaption = "Eleven evenings with a book this month."

    // MARK: Tonight screen

    static let tonightChips = ["Fiction", "Essays", "Night reads"]
}
