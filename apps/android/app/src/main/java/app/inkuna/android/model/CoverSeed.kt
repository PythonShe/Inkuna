package app.inkuna.android.model

/**
 * A stable per-book seed for the generated cover.
 *
 * Deliberately not [String.hashCode]: FNV-1a keeps the colour of a book
 * identical across processes, so the same book wears the same cover on
 * every screen that draws it. Cross-shell identity is not a goal — ids are
 * minted per device, so the two shells never see the same id anyway.
 *
 * The generated cover is also the fallback under real cover art: `BookCover`
 * keeps drawing it until `Publication.coverPath` decodes, and forever for a
 * book whose file carried no cover.
 */
fun coverSeed(id: String): Int {
    var hash = -0x340d631b7bdddcdbL // FNV-1a 64-bit offset basis
    for (byte in id.toByteArray()) {
        hash = hash xor (byte.toLong() and 0xff)
        hash *= 0x100000001b3L
    }
    return (hash ushr 1).toInt() and Int.MAX_VALUE
}
