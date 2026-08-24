// The UniFFI-generated bindings (Generated/InkunaCore.swift) are compiled
// directly into this target, so core types need no import.
import Foundation

/// Reading positions, asked of the core rather than computed here.
///
/// Synthetic positions are a core derivation: how many a book has, and
/// which one a coordinate lands on, both depend on the engine's canonical
/// projection. The shells only ever ask. Nothing in this file may grow a
/// formula — no positions-per-resource constant, no offset arithmetic —
/// because the same answers have to come out on both platforms.
///
/// Android sibling: `ui/reader/ReaderPositions.kt`.
enum ReaderPositions {
    /// The 1-based synthetic position `coordinate` sits on.
    ///
    /// Throws when the book is unknown to the core; callers hide the
    /// position line rather than showing a guessed one.
    static func position(
        of coordinate: Coordinate,
        id: String,
        on shelf: Bookshelf
    ) async throws -> UInt32 {
        try await shelf.progress().positionOf(id: id, coordinate: coordinate)
    }

    /// How far into the book `coordinate` sits, in `0...1`.
    ///
    /// Derived from the position and the book's position count, so it
    /// agrees with the "p. N of M" line by construction. A book with no
    /// positions yet reads as 0 rather than dividing by zero.
    static func progression(
        of coordinate: Coordinate,
        id: String,
        on shelf: Bookshelf
    ) async throws -> Double {
        let progress = shelf.progress()
        let count = try await progress.positionCount(id: id)
        guard count > 0 else { return 0 }
        let position = try await progress.positionOf(id: id, coordinate: coordinate)
        return min(max(Double(position) / Double(count), 0), 1)
    }

    /// The chapter range holding `position`, or nil when none does.
    ///
    /// Ranges are 1-based and inclusive, and several may contain one
    /// position when a resource carries nested TOC entries — positions are
    /// resource-granular and cannot split inside one. The innermost
    /// (greatest start) wins; equal starts — fragment-anchored entries
    /// sharing a resource — tie-break on greatest `chapterIdx`, so the
    /// deepest (last-listed) entry beats its parent.
    ///
    /// The set is sparse: the core emits one range per TOC chapter, keyed
    /// by `chapterIdx`, and never one per spine resource — a resource with
    /// no TOC entry of its own falls outside every range and reads as nil
    /// here. Shared so the home and detail screens cannot drift from each
    /// other; the reader's contents sheet highlights by a looser rule of
    /// its own and is not built on this.
    static func chapterRange(
        in ranges: [ChapterPositionRange],
        at position: UInt32
    ) -> ChapterPositionRange? {
        ranges
            .filter { $0.startPosition <= position && position <= $0.endPosition }
            .max { ($0.startPosition, $0.chapterIdx) < ($1.startPosition, $1.chapterIdx) }
    }
}
