package app.inkuna.android.model

/**
 * A stable per-book seed for the generated cover.
 *
 * Deliberately not [String.hashCode]: FNV-1a keeps the colour of a book
 * identical across processes and platforms, so the same book wears the same
 * cover on every screen that draws it and on both shells.
 *
 * TODO(core): the core already extracts real cover art into
 * `Publication.coverPath`; render it once `BookCover` accepts an image and
 * falls back to the generated cover.
 */
fun coverSeed(id: String): Int {
    var hash = -0x340d631b7bdddcdbL // FNV-1a 64-bit offset basis
    for (byte in id.toByteArray()) {
        hash = hash xor (byte.toLong() and 0xff)
        hash *= 0x100000001b3L
    }
    return (hash ushr 1).toInt() and Int.MAX_VALUE
}
