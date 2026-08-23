package app.inkuna.android.ui.reader.engine

import android.graphics.fonts.Font
import android.util.Log
import app.inkuna.core.FontEntry
import java.io.File
import java.io.RandomAccessFile
import java.nio.charset.Charset

/** Rebuilds the exact engine font faces for [android.graphics.Canvas.drawGlyphs]. */
object ReaderFontStore {
    private const val TAG = "InkunaReaderFonts"

    @Volatile
    private var fonts: Map<UInt, Font> = emptyMap()

    /**
     * Replaces the whole immutable registry at once, so drawing threads only
     * ever observe a complete font map.
     */
    fun prime(registry: List<FontEntry>) {
        val rebuilt = buildMap {
            registry.forEach { entry ->
                val font = runCatching {
                    Font.Builder(File(entry.filePath))
                        .setTtcIndex(entry.collectionIndex.toInt())
                        .apply {
                            if (entry.axes.isNotEmpty()) {
                                setFontVariationSettings(
                                    entry.axes.joinToString { "'${it.tag}' ${it.value}" },
                                )
                            }
                        }
                        .build()
                }.getOrElse { error ->
                    Log.e(TAG, "Unable to load reader font ${entry.id} from ${entry.filePath}", error)
                    return@forEach
                }

                val fileMatches = font.file?.canonicalFile == File(entry.filePath).canonicalFile
                val indexMatches = font.ttcIndex == entry.collectionIndex.toInt()
                val resolvedName = PostScriptName.read(File(entry.filePath), entry.collectionIndex.toInt())
                if (!fileMatches || !indexMatches || resolvedName != entry.postScriptName) {
                    Log.e(
                        TAG,
                        "Rejecting reader font ${entry.id}: TTC face ${entry.collectionIndex} " +
                            "resolved ${resolvedName ?: "<missing>"}, expected ${entry.postScriptName}",
                    )
                    return@forEach
                }
                put(entry.id, font)
            }
        }
        fonts = rebuilt
    }

    fun font(id: UInt): Font? = fonts[id]

    /** Android's Font API exposes the file and TTC index but not name ID 6. */
    private object PostScriptName {
        fun read(file: File, collectionIndex: Int): String? = runCatching {
            RandomAccessFile(file, "r").use { reader ->
                val fontOffset = sfntOffset(reader, collectionIndex)
                reader.seek(fontOffset + 4)
                val tableCount = reader.readUnsignedShort()
                reader.skipBytes(6)
                var nameTableOffset: Long? = null
                repeat(tableCount) {
                    val tag = reader.readTag()
                    reader.skipBytes(4)
                    val offset = reader.readUnsignedInt()
                    reader.skipBytes(4)
                    if (tag == "name") nameTableOffset = offset
                }
                nameTableOffset?.let { readNameTable(reader, it) }
            }
        }.getOrNull()

        private fun sfntOffset(reader: RandomAccessFile, collectionIndex: Int): Long {
            reader.seek(0)
            if (reader.readTag() != "ttcf") return 0
            reader.skipBytes(4)
            val count = reader.readUnsignedInt()
            require(collectionIndex >= 0 && collectionIndex.toLong() < count)
            reader.seek(12L + collectionIndex * 4L)
            return reader.readUnsignedInt()
        }

        private fun readNameTable(reader: RandomAccessFile, tableOffset: Long): String? {
            reader.seek(tableOffset + 2)
            val count = reader.readUnsignedShort()
            val stringsOffset = reader.readUnsignedShort().toLong()
            val recordsOffset = tableOffset + 6
            var fallback: NameRecord? = null
            repeat(count) {
                reader.seek(recordsOffset + it * 12L)
                val record = NameRecord(
                    platform = reader.readUnsignedShort(),
                    encoding = reader.readUnsignedShort(),
                    language = reader.readUnsignedShort(),
                    nameId = reader.readUnsignedShort(),
                    length = reader.readUnsignedShort(),
                    offset = reader.readUnsignedShort(),
                )
                if (record.nameId == POST_SCRIPT_NAME) {
                    if (record.platform == WINDOWS_PLATFORM || record.platform == UNICODE_PLATFORM) {
                        return decode(reader, tableOffset + stringsOffset, record)
                    }
                    fallback = record
                }
            }
            return fallback?.let { decode(reader, tableOffset + stringsOffset, it) }
        }

        private fun decode(reader: RandomAccessFile, stringsOffset: Long, record: NameRecord): String? {
            val bytes = ByteArray(record.length)
            reader.seek(stringsOffset + record.offset)
            reader.readFully(bytes)
            val charset = when (record.platform) {
                WINDOWS_PLATFORM, UNICODE_PLATFORM -> Charsets.UTF_16BE
                MACINTOSH_PLATFORM -> MAC_ROMAN
                else -> return null
            }
            return bytes.toString(charset).trimEnd('\u0000')
        }

        private fun RandomAccessFile.readTag(): String = ByteArray(4).also(::readFully)
            .toString(Charsets.ISO_8859_1)

        private fun RandomAccessFile.readUnsignedInt(): Long = readInt().toLong() and 0xFFFF_FFFFL

        private data class NameRecord(
            val platform: Int,
            val encoding: Int,
            val language: Int,
            val nameId: Int,
            val length: Int,
            val offset: Int,
        )

        private const val POST_SCRIPT_NAME = 6
        private const val UNICODE_PLATFORM = 0
        private const val MACINTOSH_PLATFORM = 1
        private const val WINDOWS_PLATFORM = 3
        private val MAC_ROMAN = Charset.forName("x-MacRoman")
    }
}
