import Foundation

// Placeholder library content matching the design canvas, so every screen
// renders exactly as designed before the Rust core is wired in.
//
// TODO(core): replace with `LibraryStore.shared.library()` (`Bookshelf`)
// queries — book list, chapters/TOC, positions, and reading stats all come
// from the core; none of this data belongs in the shell.

struct PlaceholderBook: Hashable, Identifiable {
    let id: Int
    let title: String
    let author: String
    /// 0…1 fraction read.
    let progress: Double
    let currentPage: Int
    let pageCount: Int
    let coverSeed: Int
    let downloaded: Bool

    var percentText: String { "\(Int((progress * 100).rounded()))%" }
}

enum PlaceholderLibrary {
    static let books: [PlaceholderBook] = [
        PlaceholderBook(id: 0, title: "The Will of the Many", author: "James Islington", progress: 0.40, currentPage: 564, pageCount: 1418, coverSeed: 0, downloaded: true),
        PlaceholderBook(id: 1, title: "Letters at Dusk", author: "I. Sato", progress: 0.64, currentPage: 198, pageCount: 310, coverSeed: 2, downloaded: true),
        PlaceholderBook(id: 2, title: "Jade City", author: "Fonda Lee", progress: 0.17, currentPage: 87, pageCount: 512, coverSeed: 3, downloaded: true),
        PlaceholderBook(id: 3, title: "West with the Night", author: "Beryl Markham", progress: 0.33, currentPage: 96, pageCount: 294, coverSeed: 1, downloaded: false),
        PlaceholderBook(id: 4, title: "The Quiet Codex", author: "A. Moreno", progress: 0.05, currentPage: 21, pageCount: 402, coverSeed: 4, downloaded: false),
        PlaceholderBook(id: 5, title: "A Winter Ledger", author: "H. Brandt", progress: 0.01, currentPage: 4, pageCount: 356, coverSeed: 5, downloaded: true),
    ]

    /// The book on the Tonight hero card.
    static var heroBook: PlaceholderBook { books[0] }

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
