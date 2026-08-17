import Foundation

// The last placeholder content standing, each entry waiting on a core
// capability that does not exist yet: the tonight chips on collections,
// the hero stand-in on nothing — it renders only in an empty library and
// is inert by design.

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

    /// The stand-in card's caption, invented like the rest of that card;
    /// a real book's caption comes from the core.
    static var pagesLeftText: String {
        String(localized: "tonight_pages_left", defaultValue: "Fifteen pages left in this chapter")
    }

    // MARK: Stats screen

    static var facts: [(value: String, caption: String)] {
        let halfFormat = NSLocalizedString("stats_hours_half", comment: "")
        let sixAndHalf = String.localizedStringWithFormat(halfFormat, "6")
        return [
            ("214", String(localized: "stats_pages_this_week", defaultValue: "pages this week")),
            (sixAndHalf, String(localized: "stats_hours_this_month", defaultValue: "hours this month")),
            ("12", String(localized: "stats_books_this_year", defaultValue: "books this year")),
        ]
    }

    static let calendarMonthTitle = "August"
    /// Weekday column the 1st falls on (0 = Sunday).
    static let calendarLeadingBlanks = 6
    static let calendarDayCount = 31
    static let calendarToday = 14
    static let calendarReadDays: Set<Int> = [1, 2, 3, 5, 8, 9, 10, 11, 13, 14]
    static var calendarCaption: String {
        let format = NSLocalizedString("stats_evenings", comment: "")
        return String.localizedStringWithFormat(format, Int64(11))
    }

    // MARK: Tonight screen

    static var tonightChips: [String] {
        [
            String(localized: "tonight_chip_fiction", defaultValue: "Fiction"),
            String(localized: "tonight_chip_essays", defaultValue: "Essays"),
            String(localized: "tonight_chip_night_reads", defaultValue: "Night reads"),
        ]
    }
}
