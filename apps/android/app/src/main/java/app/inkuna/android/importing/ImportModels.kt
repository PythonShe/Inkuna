package app.inkuna.android.importing

import androidx.compose.runtime.Immutable
import app.inkuna.core.InkunaException
import app.inkuna.core.Publication
import java.util.Locale

/**
 * A book the core accepted or already held.
 *
 * The generated [Publication] is a `var`-field data class, which Compose
 * treats as unstable; this is the immutable projection the UI reads, made
 * once at the boundary.
 */
@Immutable
data class ImportedBook(
    val id: String,
    val title: String,
    val authors: List<String>,
) {
    companion object {
        fun of(publication: Publication) = ImportedBook(
            id = publication.id,
            title = publication.title,
            authors = publication.authors.toList(),
        )
    }
}

/**
 * Why one file did not become a book.
 *
 * The batch path flattens the core's typed `InkunaError` into a string
 * (`ImportOutcome.Failed.message` is `error.to_string()`), so the variant
 * has to be recovered from the message prefix — those prefixes are fixed by
 * `CoreError`'s `#[error(...)]` attributes and are effectively part of the
 * contract. [ImportFailure.of] is the single place that mapping lives.
 */
enum class ImportFailureKind {
    /** Recognized, but not importable yet — MOBI, PDF, CBZ, CBR, TXT. */
    UnsupportedFormat,

    /** Nothing recognizable at all: not a book. */
    UnknownFormat,

    /** An EPUB the core could not turn into a publication. */
    BrokenBook,

    /** The container is damaged. */
    DamagedArchive,

    /** The file could not be read: revoked permission, storage error, empty. */
    Unreadable,

    /** The library database itself is in trouble. */
    LibraryError,
}

@Immutable
data class ImportFailure(
    val name: String,
    val kind: ImportFailureKind,
    /** The named format for [ImportFailureKind.UnsupportedFormat], e.g. `"MOBI"`. */
    val format: String?,
    /** The core's own message, for logs — never shown as-is. */
    val detail: String,
) {
    companion object {
        private const val UNSUPPORTED = "unsupported format"
        private const val INVALID = "invalid publication"
        private const val ARCHIVE = "archive error"
        private const val IO = "io error"
        private const val DATABASE = "database error"
        private const val NOT_FOUND = "publication not found"

        /** Classifies a `ImportOutcome.Failed.message` from the batch path. */
        fun of(name: String, message: String): ImportFailure {
            val lower = message.lowercase(Locale.ROOT)
            return when {
                lower.startsWith(UNSUPPORTED) -> {
                    val named = message.substringAfter(':', "").trim().takeIf { it.isNotEmpty() }
                    ImportFailure(
                        name = name,
                        kind = if (named == null) ImportFailureKind.UnknownFormat
                        else ImportFailureKind.UnsupportedFormat,
                        format = named?.uppercase(Locale.ROOT),
                        detail = message,
                    )
                }
                lower.startsWith(INVALID) -> failure(name, ImportFailureKind.BrokenBook, message)
                lower.startsWith(ARCHIVE) -> failure(name, ImportFailureKind.DamagedArchive, message)
                lower.startsWith(IO) -> failure(name, ImportFailureKind.Unreadable, message)
                lower.startsWith(DATABASE) || lower.startsWith(NOT_FOUND) ->
                    failure(name, ImportFailureKind.LibraryError, message)
                else -> failure(name, ImportFailureKind.BrokenBook, message)
            }
        }

        /**
         * Classifies a thrown failure — the whole-batch throw and anything
         * the shell's own staging step raises.
         *
         * `InkunaError` is a UniFFI `flat_error`, so a thrown
         * [InkunaException] already carries the same Display string the
         * batch path puts in `Failed.message`; it goes through the same
         * classifier rather than a parallel `when` that could drift.
         */
        fun of(name: String, error: Throwable): ImportFailure =
            if (error is InkunaException) {
                of(name, describe(error))
            } else {
                failure(name, ImportFailureKind.Unreadable, describe(error))
            }

        private fun describe(error: Throwable) =
            error.message?.takeIf { it.isNotBlank() } ?: error::class.java.simpleName

        private fun failure(name: String, kind: ImportFailureKind, detail: String) =
            ImportFailure(name = name, kind = kind, format = null, detail = detail)
    }
}

/** What one import run did, in the order the reader picked the files. */
@Immutable
data class ImportReport(
    val added: List<ImportedBook>,
    val duplicates: List<ImportedBook>,
    val failures: List<ImportFailure>,
    val cancelled: Boolean,
) {
    val total: Int get() = added.size + duplicates.size + failures.size
    val changedLibrary: Boolean get() = added.isNotEmpty()

    companion object {
        val Empty = ImportReport(emptyList(), emptyList(), emptyList(), cancelled = false)
    }
}

/** The two phases a reader can actually perceive. */
enum class ImportPhase {
    /** Copying the picked document out of its provider into app cache. */
    Copying,

    /** The core is hashing, deduping, parsing and committing. */
    Reading,
}

@Immutable
sealed interface ImportState {
    /** Nothing running and nothing to report. */
    data object Idle : ImportState

    /**
     * @param completed files fully dealt with so far.
     * @param total files in this selection.
     * @param fraction overall progress in `0..1`, or `null` while the core
     *   works and cannot report any (`import_batch` has no callback).
     */
    @Immutable
    data class Running(
        val completed: Int,
        val total: Int,
        val currentName: String,
        val phase: ImportPhase,
        val fraction: Float?,
    ) : ImportState

    @Immutable
    data class Finished(val report: ImportReport) : ImportState
}
