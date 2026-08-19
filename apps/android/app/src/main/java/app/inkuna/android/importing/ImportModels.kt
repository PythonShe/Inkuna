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
 * Both failure paths carry the same typed error since the FFI stopped
 * flattening it: a batch item's `ImportOutcome.Failed.error` and a thrown
 * [InkunaException] are classified by variant, never by message text.
 * [ImportFailure.of] is the single place that mapping lives.
 */
enum class ImportFailureKind {
    /** Recognized, but not importable yet — PDF, CBZ, CBR. */
    UnsupportedFormat,

    /** Nothing recognizable at all: not a book. */
    UnknownFormat,

    /** An EPUB the core could not turn into a publication. */
    BrokenBook,

    /** The container is damaged. */
    DamagedArchive,

    /** Past the import ceiling: refused mid-copy rather than left to fill the device. */
    TooLarge,

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
        /**
         * Classifies the typed error a batch `Failed` item carries — the
         * same [InkunaException] the single-file path throws, so both
         * paths route through this one exhaustive `when`.
         */
        fun of(name: String, error: InkunaException): ImportFailure = when (error) {
            is InkunaException.UnsupportedFormat -> ImportFailure(
                name = name,
                kind = if (error.format == null) ImportFailureKind.UnknownFormat
                else ImportFailureKind.UnsupportedFormat,
                format = error.format?.uppercase(Locale.ROOT),
                detail = describe(error),
            )
            is InkunaException.InvalidPublication ->
                failure(name, ImportFailureKind.BrokenBook, describe(error))
            is InkunaException.Archive ->
                failure(name, ImportFailureKind.DamagedArchive, describe(error))
            is InkunaException.FileTooLarge ->
                failure(name, ImportFailureKind.TooLarge, describe(error))
            is InkunaException.Io ->
                failure(name, ImportFailureKind.Unreadable, describe(error))
            // The search index is derived data the core writes alongside
            // the book; a failure there is the library in trouble, not the
            // file — the same thing the reader needs to hear.
            is InkunaException.Database,
            is InkunaException.NotFound,
            is InkunaException.Search,
            is InkunaException.InvalidPositionRanges,
            ->
                failure(name, ImportFailureKind.LibraryError, describe(error))
        }

        /**
         * Classifies a thrown failure — the whole-batch throw and anything
         * the shell's own staging step raises. A thrown [InkunaException]
         * goes through the typed classifier above rather than a parallel
         * `when` that could drift.
         */
        fun of(name: String, error: Throwable): ImportFailure =
            if (error is InkunaException) {
                of(name, error)
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
    /**
     * Fallback only: copying a stream-only virtual document into cache
     * before it can travel to the core as a descriptor. Most files skip
     * this phase entirely.
     */
    Copying,

    /** The core is streaming, hashing, deduping, parsing and committing. */
    Reading,
}

@Immutable
sealed interface ImportState {
    /** Nothing running and nothing to report. */
    data object Idle : ImportState

    /**
     * @param completed files fully dealt with so far.
     * @param total files in this selection.
     * @param fraction overall progress in `0..1`, or `null` when nothing
     *   is known yet.
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
