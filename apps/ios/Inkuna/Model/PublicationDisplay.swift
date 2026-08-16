import Foundation

extension Publication {
    /// The author line every screen shows: the core's authors joined, or the
    /// caller's stand-in for a book that names none — the user-visible
    /// string stays at screen level, where the l10n pass reaches it.
    func displayAuthors(unknownAuthor: String) -> String {
        authors.isEmpty ? unknownAuthor : authors.joined(separator: ", ")
    }
}
