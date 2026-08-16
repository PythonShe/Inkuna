import Foundation

extension Publication {
    /// The author line every screen shows: the core's authors joined, or
    /// the stand-in for a book that names none.
    // TODO(l10n): localize once the strings pass lands.
    var displayAuthors: String {
        authors.isEmpty ? "Unknown author" : authors.joined(separator: ", ")
    }
}
