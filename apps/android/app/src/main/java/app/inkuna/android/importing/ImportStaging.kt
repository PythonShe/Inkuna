package app.inkuna.android.importing

import android.content.ContentResolver
import android.content.Context
import android.net.Uri
import android.provider.OpenableColumns
import android.util.Log
import app.inkuna.android.R
import app.inkuna.core.InkunaException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import java.io.File
import java.io.FileNotFoundException
import java.io.IOException
import java.util.UUID
import kotlin.coroutines.coroutineContext

/**
 * One picked document, copied into app cache.
 *
 * @param file the cache copy; deleted the moment the chunk that made it
 *   is done.
 * @param displayName the provider's name for the document, kept verbatim
 *   (CJK included) so failures and duplicates can be reported by the name
 *   the reader actually saw in the picker.
 */
internal class StagedDocument(
    val file: File,
    val displayName: String,
    val source: Uri,
)

/**
 * The staging fallback.
 *
 * Most picked documents travel to the core as file descriptors and never
 * touch this. A *virtual* document — one whose provider can produce a
 * stream but not a descriptor — is copied to
 * `cacheDir/epub-import/<run>/<name>` first, imported from a descriptor to
 * that copy, and deleted immediately afterwards — successful, failed, or
 * cancelled. A whole run is a [ImportRun], which owns its directory and
 * removes it on close.
 *
 * The core takes the display name separately from the bytes, so the staged
 * name only has to be honest enough for a human to recognize in the cache.
 */
internal object ImportStaging {
    private const val TAG = "ImportStaging"
    private const val ROOT = "epub-import"

    /** 64 KiB: large enough that a 50 MB book is ~800 reads, small enough to stay off the LOB heap. */
    private const val COPY_BUFFER = 64 * 1024

    /** Report copy progress at most this often, so a fast copy can't flood recomposition. */
    private const val PROGRESS_INTERVAL_MS = 60L

    /**
     * The core's own import ceiling (`files::MAX_IMPORT_BYTES`, 2 GiB),
     * mirrored here so the *first* boundary a hostile or broken provider
     * crosses is the one that stops it: this fallback writes into the app
     * cache before the core ever sees a byte, and the size a provider
     * declares is not something to trust. The core enforces the same limit
     * on what it stores, so the two agree on which files are books; this
     * only saves the wasted cache write.
     */
    private const val MAX_IMPORT_BYTES = 2L * 1024 * 1024 * 1024

    /**
     * ext4/f2fs cap file *names* at 255 bytes. A CJK title is 3 bytes per
     * character in UTF-8, so a character budget would overflow — truncation
     * is by encoded length, on code-point boundaries.
     */
    private const val MAX_NAME_BYTES = 200

    /**
     * Run directories currently in use. A sweep must not delete a sibling
     * run's staging area — two imports can be live at once (the library
     * screen and an inbound `ACTION_VIEW`).
     */
    private val activeRuns = mutableSetOf<String>()

    private fun root(context: Context) = File(context.cacheDir, ROOT)

    /**
     * Opens a staging directory for one import run and removes anything a
     * previous, killed process left behind.
     */
    fun openRun(context: Context): ImportRun {
        val name = "run-${UUID.randomUUID()}"
        val dir = File(root(context), name)
        synchronized(activeRuns) { activeRuns += name }
        dir.mkdirs()
        sweepOrphans(context)
        return ImportRun(name, dir)
    }

    /** Deletes staging directories no live run owns — leftovers from a crash or a kill. */
    private fun sweepOrphans(context: Context) {
        val live = synchronized(activeRuns) { activeRuns.toSet() }
        root(context).listFiles()?.forEach { child ->
            if (child.name !in live) {
                runCatching { child.deleteRecursively() }
                    .onFailure { Log.w(TAG, "Could not sweep stale staging dir ${child.name}", it) }
            }
        }
    }

    internal fun releaseRun(run: ImportRun) {
        synchronized(activeRuns) { activeRuns -= run.name }
    }

    /**
     * Copies [uri] into [run]'s directory.
     *
     * Cancellation-safe: the loop checks the job on every block, and a
     * cancelled or failed copy deletes its partial file before rethrowing,
     * so nothing is ever left in the cache. A stream that runs past
     * [MAX_IMPORT_BYTES] is cut off the same way.
     *
     * @param onProgress copied bytes and the provider's declared size when
     *   it reported one (`null` when it did not).
     */
    suspend fun stage(
        context: Context,
        uri: Uri,
        run: ImportRun,
        displayName: String,
        onProgress: (copied: Long, total: Long?) -> Unit,
    ): StagedDocument = withContext(Dispatchers.IO) {
        val resolver = context.contentResolver
        val declaredSize = declaredSize(resolver, uri)
        val target = File(run.dir, uniqueName(run.dir, safeFileName(displayName)))
        try {
            val stream = resolver.openInputStream(uri)
                ?: throw FileNotFoundException("The file provider returned nothing for $uri")
            stream.use { input ->
                target.outputStream().use { output ->
                    val buffer = ByteArray(COPY_BUFFER)
                    var copied = 0L
                    var lastReport = 0L
                    onProgress(0L, declaredSize)
                    while (true) {
                        coroutineContext.ensureActive()
                        val read = input.read(buffer)
                        if (read < 0) break
                        copied += read
                        if (copied > MAX_IMPORT_BYTES) {
                            // Refused with the core's own typed error, so a
                            // stream cut off here reads exactly like one the
                            // core cut off itself.
                            throw InkunaException.FileTooLarge(MAX_IMPORT_BYTES.toULong())
                        }
                        output.write(buffer, 0, read)
                        val now = System.currentTimeMillis()
                        if (now - lastReport >= PROGRESS_INTERVAL_MS) {
                            lastReport = now
                            onProgress(copied, declaredSize)
                        }
                    }
                    output.flush()
                    onProgress(copied, declaredSize)
                }
            }
        } catch (t: Throwable) {
            target.delete()
            throw t
        }
        if (target.length() == 0L) {
            target.delete()
            throw IOException("The file is empty")
        }
        StagedDocument(target, displayName, uri)
    }

    /**
     * The provider's display name for [uri], or a name derived from the URI
     * when it exposes none. Returned verbatim — a Chinese or Japanese
     * filename has to survive intact, because it is both the label the
     * reader sees and (via the staged copy) the core's fallback book title
     * for an EPUB whose OPF carries none.
     */
    fun displayName(context: Context, uri: Uri): String {
        val fromProvider = runCatching {
            context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
                ?.use { cursor ->
                    val column = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                    if (column >= 0 && cursor.moveToFirst()) cursor.getString(column) else null
                }
        }.getOrNull()
        fromProvider?.trim()?.takeIf { it.isNotBlank() }?.let { return it }
        // Last resort: the URI's own tail, but only when it actually looks
        // like a filename. A `content://media/external/file/55` tail is a row
        // id, and reporting "55 couldn't be added" tells the reader nothing.
        val tail = uri.lastPathSegment?.substringAfterLast('/')?.trim()
        return if (tail != null && tail.contains('.') && tail.any { !it.isDigit() }) {
            tail
        } else {
            context.getString(R.string.import_unnamed_file)
        }
    }

    private fun declaredSize(resolver: ContentResolver, uri: Uri): Long? = runCatching {
        resolver.query(uri, arrayOf(OpenableColumns.SIZE), null, null, null)?.use { cursor ->
            val column = cursor.getColumnIndex(OpenableColumns.SIZE)
            if (column >= 0 && cursor.moveToFirst() && !cursor.isNull(column)) {
                cursor.getLong(column).takeIf { it > 0 }
            } else {
                null
            }
        }
    }.getOrNull()

    /**
     * Strips only what a filesystem cannot carry — separators, NUL, and
     * leading dots — then truncates to [MAX_NAME_BYTES] UTF-8 bytes on a
     * code-point boundary, keeping the extension.
     */
    internal fun safeFileName(raw: String): String {
        val cleaned = raw
            // Separators and control characters only: CJK is left untouched.
            .map { c -> if (c == '/' || c == '\\' || c.code < 0x20 || c.code == 0x7F) '_' else c }
            .joinToString("")
            .trim()
            .trimStart('.')
            // Neutral: this names only the cache copy — the core receives the
            // display name separately and detects format by content, so no
            // extension is implied here.
            .ifBlank { "book.bin" }
        if (cleaned.toByteArray(Charsets.UTF_8).size <= MAX_NAME_BYTES) return cleaned

        val extension = cleaned.substringAfterLast('.', "").takeIf { it.length in 1..8 }?.let { ".$it" } ?: ""
        val stemBudget = MAX_NAME_BYTES - extension.toByteArray(Charsets.UTF_8).size
        val stem = cleaned.removeSuffix(extension)
        val builder = StringBuilder()
        var bytes = 0
        // Iterate by code point so a truncated surrogate pair or a
        // half-written multi-byte character can never be produced.
        var index = 0
        while (index < stem.length) {
            val codePoint = stem.codePointAt(index)
            val chars = Character.charCount(codePoint)
            val size = String(Character.toChars(codePoint)).toByteArray(Charsets.UTF_8).size
            if (bytes + size > stemBudget) break
            builder.appendRange(stem, index, index + chars)
            bytes += size
            index += chars
        }
        return builder.toString().ifBlank { "book" } + extension
    }

    /** Disambiguates two picks with the same name inside one run. */
    private fun uniqueName(dir: File, displayName: String): String {
        if (!File(dir, displayName).exists()) return displayName
        val extension = displayName.substringAfterLast('.', "").takeIf { it.isNotEmpty() }?.let { ".$it" } ?: ""
        val stem = displayName.removeSuffix(extension)
        var suffix = 2
        while (File(dir, "$stem-$suffix$extension").exists()) suffix++
        return "$stem-$suffix$extension"
    }
}

/**
 * One import run's staging directory. Closing removes it whole — the only
 * cleanup path, used from a `NonCancellable` `finally` so a cancelled
 * import leaks nothing.
 */
internal class ImportRun(val name: String, val dir: File) : AutoCloseable {
    override fun close() {
        runCatching { dir.deleteRecursively() }
            .onFailure { Log.w("ImportStaging", "Could not remove staging dir $name", it) }
        ImportStaging.releaseRun(this)
    }
}
