import Foundation

/// The four floating-tab destinations.
enum MainTab: String, CaseIterable {
    case tonight
    case library
    case search
    case stats

    var symbol: String {
        switch self {
        case .tonight: "book"
        case .library: "books.vertical"
        case .search: "magnifyingglass"
        case .stats: "chart.bar"
        }
    }

    var title: String {
        switch self {
        case .tonight: String(localized: "tab_tonight", defaultValue: "Tonight")
        case .library: String(localized: "tab_library", defaultValue: "Library")
        case .search: String(localized: "tab_search", defaultValue: "Search")
        case .stats: String(localized: "tab_stats", defaultValue: "Stats")
        }
    }
}
