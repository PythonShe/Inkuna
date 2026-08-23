package app.inkuna.android.debug

import android.content.Context
import android.util.Log
import app.inkuna.android.model.LibraryStore
import app.inkuna.core.ImportOutcome
import app.inkuna.core.InkunaException
import app.inkuna.core.LayoutListener
import app.inkuna.core.ReaderLayoutSettings
import app.inkuna.core.Viewport
import java.io.File
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject

/** Debug-only cross-shell layout-digest harness driven entirely by its manifest. */
object ParityDigestRunner {
    private const val TAG = "InkunaParity"
    private const val BOOK_TIMEOUT_MS = 120_000L

    suspend fun run(context: Context) = withContext(Dispatchers.Default) {
        val externalFiles = context.getExternalFilesDir(null)
        if (externalFiles == null) {
            Log.e(TAG, "PARITY ERROR External files directory unavailable")
            return@withContext
        }

        val corpus = File(externalFiles, "ParityCorpus")
        val manifest = File(corpus, "manifest.json")
        try {
            val cases = parseManifest(manifest)
            val output = cases.map { item ->
                File(item.file).name to digestWithTimeout(context.applicationContext, corpus, item)
            }
            File(externalFiles, "parity-android.json").writeText(encodeOutput(output))
            Log.i(TAG, "PARITY DONE ${cases.size}")
        } catch (failure: Throwable) {
            Log.e(TAG, "PARITY ERROR ${errorMessage(failure)}", failure)
        }
    }

    private suspend fun digestWithTimeout(
        context: Context,
        corpus: File,
        item: ParityManifestCase,
    ): ParityBookValue {
        val result = CompletableDeferred<ParityBookValue>()
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        scope.launch {
            result.complete(digest(context, corpus, item))
        }
        scope.launch {
            delay(BOOK_TIMEOUT_MS)
            result.complete(ParityBookValue.StringValue("TIMEOUT"))
        }
        return try {
            result.await()
        } finally {
            scope.cancel()
        }
    }

    private suspend fun digest(
        context: Context,
        corpus: File,
        item: ParityManifestCase,
    ): ParityBookValue {
        val book = File(corpus, item.file)
        if (!book.isFile) return ParityBookValue.StringValue("ERROR: File not found: ${item.file}")

        return try {
            val shelf = LibraryStore.bookshelf(context)
            val imported = shelf.importer().`import`(book.absolutePath)
            val publicationId = when (imported) {
                is ImportOutcome.Imported -> imported.publication.id
                is ImportOutcome.Duplicate -> imported.publication.id
                is ImportOutcome.Failed -> {
                    return ParityBookValue.StringValue("ERROR: ${errorMessage(imported.error)}")
                }
            }

            val completion = ParityCompletion()
            val session = shelf.openReader(
                publicationId,
                item.viewport.coreValue,
                item.settings.coreValue,
                ParityLayoutListener(completion),
            )
            try {
                val spineCount = session.spineCount()
                // `chapter` is cache-only: its expected NotReady result queues every
                // spine through the engine's own worker before we await terminals.
                repeat(spineCount.toInt()) { spine ->
                    runCatching { session.chapter(spine.toUInt()) }
                }
                val failedSpine = completion.waitForTerminal(spineCount)
                if (failedSpine != null) {
                    ParityBookValue.StringValue("ERROR: chapter $failedSpine failed")
                } else {
                    val chapters = mutableListOf<ParityChapterDigest>()
                    for (spine in 0 until spineCount.toInt()) {
                        val spineIdx = spine.toUInt()
                        while (true) {
                            val terminalEvents = completion.terminalEventCount(spineIdx)
                            try {
                                val chapter = session.chapter(spineIdx)
                                val pages = buildList {
                                    repeat(chapter.pageCount.toInt()) { page ->
                                        add(session.pageDigest(spineIdx, page.toUInt()))
                                    }
                                }
                                chapters += ParityChapterDigest(spineIdx, pages)
                                break
                            } catch (_: InkunaException.NotReady) {
                                if (completion.waitForTerminalEvent(spineIdx, terminalEvents)) {
                                    return ParityBookValue.StringValue("ERROR: chapter $spineIdx failed")
                                }
                            } catch (failure: Throwable) {
                                return ParityBookValue.StringValue("ERROR: ${errorMessage(failure)}")
                            }
                        }
                    }
                    ParityBookValue.Pages(chapters)
                }
            } finally {
                session.close()
            }
        } catch (failure: Throwable) {
            ParityBookValue.StringValue("ERROR: ${errorMessage(failure)}")
        }
    }

    private fun parseManifest(manifest: File): List<ParityManifestCase> {
        val source = JSONArray(manifest.readText())
        return List(source.length()) { index ->
            val item = source.getJSONObject(index)
            val viewport = item.requiredObject("viewport")
            val settings = item.requiredObject("settings")
            ParityManifestCase(
                file = item.requiredString("file"),
                viewport = ParityManifestViewport(
                    width = viewport.requiredPositiveFiniteDouble("width"),
                    height = viewport.requiredPositiveFiniteDouble("height"),
                ),
                settings = ParityManifestSettings(
                    readingFont = settings.requiredString("reading_font"),
                    readingBold = settings.requiredBoolean("reading_bold"),
                    textSizeStep = settings.requiredUByte("text_size_step"),
                    lineSpacing = settings.requiredFiniteDouble("line_spacing"),
                    letterSpacing = settings.requiredFiniteDouble("letter_spacing"),
                    wordSpacing = settings.requiredFiniteDouble("word_spacing"),
                    readingMargins = settings.requiredUInt("reading_margins"),
                ),
            )
        }
    }

    private fun encodeOutput(books: List<Pair<String, ParityBookValue>>): String = buildString {
        append('{')
        books.forEachIndexed { index, (file, value) ->
            if (index > 0) append(',')
            append(JSONObject.quote(file))
            append(':')
            append(value.jsonValue())
        }
        append('}')
    }

    private fun ParityBookValue.jsonValue(): String = when (this) {
        is ParityBookValue.StringValue -> JSONObject.quote(value)
        is ParityBookValue.Pages -> buildString {
            append('{')
            chapters.forEachIndexed { index, chapter ->
                if (index > 0) append(',')
                append(JSONObject.quote(chapter.spineIdx.toString()))
                append(':')
                append(JSONArray(chapter.digests).toString())
            }
            append('}')
        }
    }

    private fun errorMessage(error: Throwable): String = when (error) {
        is InkunaException.Io -> error.detail
        is InkunaException.FileTooLarge -> "limit=${error.limit}"
        is InkunaException.Database -> error.detail
        is InkunaException.Archive -> error.detail
        is InkunaException.UnsupportedFormat -> "format=${error.format ?: "nil"}"
        is InkunaException.InvalidPublication -> error.detail
        is InkunaException.NotReady -> error.detail
        is InkunaException.UnsupportedContent -> error.detail
        is InkunaException.LayoutBudgetExceeded -> error.detail
        is InkunaException.AnchorNotFound -> error.detail
        is InkunaException.Search -> error.detail
        is InkunaException.NotFound -> "id=${error.id}"
        else -> error.message ?: error.javaClass.simpleName
    }
}

private data class ParityManifestCase(
    val file: String,
    val viewport: ParityManifestViewport,
    val settings: ParityManifestSettings,
)

private data class ParityManifestViewport(
    val width: Double,
    val height: Double,
) {
    val coreValue: Viewport
        get() = Viewport(width, height)
}

private data class ParityManifestSettings(
    val readingFont: String,
    val readingBold: Boolean,
    val textSizeStep: UByte,
    val lineSpacing: Double,
    val letterSpacing: Double,
    val wordSpacing: Double,
    val readingMargins: UInt,
) {
    val coreValue: ReaderLayoutSettings
        get() = ReaderLayoutSettings(
            readingFont,
            readingBold,
            textSizeStep,
            lineSpacing,
            letterSpacing,
            wordSpacing,
            readingMargins,
        )
}

private data class ParityChapterDigest(
    val spineIdx: UInt,
    val digests: List<String>,
)

private sealed interface ParityBookValue {
    data class Pages(val chapters: List<ParityChapterDigest>) : ParityBookValue

    data class StringValue(val value: String) : ParityBookValue
}

private fun JSONObject.requiredObject(name: String): JSONObject = requiredValue(name) as? JSONObject
    ?: throw IllegalArgumentException("$name must be an object")

private fun JSONObject.requiredString(name: String): String = requiredValue(name) as? String
    ?: throw IllegalArgumentException("$name must be a string")

private fun JSONObject.requiredBoolean(name: String): Boolean = requiredValue(name) as? Boolean
    ?: throw IllegalArgumentException("$name must be a boolean")

private fun JSONObject.requiredPositiveFiniteDouble(name: String): Double {
    val value = requiredFiniteDouble(name)
    require(value > 0) { "$name must be positive" }
    return value
}

private fun JSONObject.requiredFiniteDouble(name: String): Double {
    val value = requiredValue(name) as? Number
        ?: throw IllegalArgumentException("$name must be a number")
    val number = value.toDouble()
    require(number.isFinite()) { "$name must be finite" }
    return number
}

private fun JSONObject.requiredUByte(name: String): UByte {
    val value = requiredInteger(name)
    require(value in UByte.MIN_VALUE.toLong()..UByte.MAX_VALUE.toLong()) { "$name is out of range" }
    return value.toUByte()
}

private fun JSONObject.requiredUInt(name: String): UInt {
    val value = requiredInteger(name)
    require(value in 0L..UInt.MAX_VALUE.toLong()) { "$name is out of range" }
    return value.toUInt()
}

private fun JSONObject.requiredInteger(name: String): Long = when (val value = requiredValue(name)) {
    is Int -> value.toLong()
    is Long -> value
    else -> throw IllegalArgumentException("$name must be an integer")
}

private fun JSONObject.requiredValue(name: String): Any {
    val value = get(name)
    require(value !== JSONObject.NULL) { "$name must not be null" }
    return value
}

private class ParityCompletion {
    private val terminal = linkedSetOf<UInt>()
    private val terminalEventCounts = mutableMapOf<UInt, Int>()
    private val terminalEventFailures = mutableMapOf<UInt, Boolean>()
    private val terminalEventWaiters = mutableMapOf<UInt, CompletableDeferred<Boolean>>()
    private var failedSpine: UInt? = null
    private var expected: UInt? = null
    private val completion = CompletableDeferred<UInt?>()

    fun chapterReady(spineIdx: UInt) = record(spineIdx, failed = false)

    fun chapterFailed(spineIdx: UInt) = record(spineIdx, failed = true)

    suspend fun waitForTerminal(spineCount: UInt): UInt? {
        synchronized(this) {
            expected = spineCount
            completeIfTerminal()
        }
        return completion.await()
    }

    fun terminalEventCount(spineIdx: UInt): Int = synchronized(this) {
        terminalEventCounts[spineIdx] ?: 0
    }

    suspend fun waitForTerminalEvent(spineIdx: UInt, after: Int): Boolean {
        val waiter = CompletableDeferred<Boolean>()
        var immediate: Boolean? = null
        synchronized(this) {
            if ((terminalEventCounts[spineIdx] ?: 0) > after) {
                immediate = terminalEventFailures[spineIdx] ?: false
            } else {
                terminalEventWaiters[spineIdx] = waiter
            }
        }
        return immediate ?: waiter.await()
    }

    private fun record(spineIdx: UInt, failed: Boolean) {
        synchronized(this) {
            terminalEventCounts[spineIdx] = (terminalEventCounts[spineIdx] ?: 0) + 1
            terminalEventFailures[spineIdx] = failed
            terminalEventWaiters.remove(spineIdx)?.complete(failed)
            if (!terminal.add(spineIdx)) return
            if (failed) failedSpine = minOf(failedSpine ?: spineIdx, spineIdx)
            completeIfTerminal()
        }
    }

    private fun completeIfTerminal() {
        val expected = expected ?: return
        if (terminal.size >= expected.toInt()) completion.complete(failedSpine)
    }
}

private class ParityLayoutListener(
    private val completion: ParityCompletion,
) : LayoutListener {
    override fun onFirstPageReady(generation: ULong, spineIdx: UInt) = Unit

    override fun onChapterReady(generation: ULong, spineIdx: UInt, pageCount: UInt) {
        completion.chapterReady(spineIdx)
    }

    override fun onChapterFailed(generation: ULong, spineIdx: UInt) {
        completion.chapterFailed(spineIdx)
    }
}
