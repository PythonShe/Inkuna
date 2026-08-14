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

    // TODO(l10n): localize once the strings pass lands.
    var title: String {
        switch self {
        case .tonight: "Tonight"
        case .library: "Library"
        case .search: "Search"
        case .stats: "Stats"
        }
    }
}
