package app.inkuna.android.ui.reader

import app.inkuna.core.Bookshelf
import app.inkuna.core.ChapterPositionRange
import app.inkuna.core.Coordinate

/**
 * Reading positions, asked of the core rather than computed here.
 *
 * Synthetic positions are a core derivation: how many a book has, and
 * which one a coordinate lands on, both depend on the engine's canonical
 * projection. The shells only ever ask. Nothing in this file may grow a
 * formula — no positions-per-resource constant, no offset arithmetic —
 * because the same answers have to come out on both platforms.
 *
 * iOS sibling: `apps/ios/Inkuna/Model/ReaderPositions.swift`.
 */
object ReaderPositions {

    /**
     * The 1-based synthetic position [coordinate] sits on.
     *
     * Throws when the book is unknown to the core; callers hide the
     * position line rather than showing a guessed one.
     */
    suspend fun position(coordinate: Coordinate, id: String, shelf: Bookshelf): UInt =
        shelf.progress().positionOf(id, coordinate)

    /**
     * How far into the book [coordinate] sits, in `0..1`.
     *
     * Derived from the position and the book's position count, so it
     * agrees with the "p. N of M" line by construction. A book with no
     * positions yet reads as 0 rather than dividing by zero.
     */
    suspend fun progression(coordinate: Coordinate, id: String, shelf: Bookshelf): Double {
        val progress = shelf.progress()
        val count = progress.positionCount(id)
        if (count == 0u) return 0.0
        val position = progress.positionOf(id, coordinate)
        return (position.toDouble() / count.toDouble()).coerceIn(0.0, 1.0)
    }

    /**
     * The chapter range holding [position], or null when none does.
     *
     * Ranges are 1-based and inclusive, and several may contain one
     * position when a resource carries nested TOC entries — positions are
     * resource-granular and cannot split inside one. The innermost
     * (greatest start) wins.
     *
     * The set is sparse: the core emits one range per TOC chapter, keyed
     * by [ChapterPositionRange.chapterIdx], and never one per spine
     * resource — a resource with no TOC entry of its own falls outside
     * every range and reads as null here. Shared so the home and detail
     * screens cannot drift from each other; the reader's contents sheet
     * highlights by a looser rule of its own and is not built on this.
     */
    fun chapterRange(ranges: List<ChapterPositionRange>, position: UInt): ChapterPositionRange? =
        ranges
            .filter { it.startPosition <= position && position <= it.endPosition }
            .maxByOrNull { it.startPosition }
}
