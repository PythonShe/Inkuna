package app.inkuna.android.model

import android.content.Context
import android.util.Log
import androidx.datastore.core.handlers.ReplaceFileCorruptionHandler
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.emptyPreferences
import androidx.datastore.preferences.core.floatPreferencesKey
import androidx.datastore.preferences.core.intPreferencesKey
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import app.inkuna.android.ui.theme.ReadingTheme
import app.inkuna.core.Settings
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.updateAndGet
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import java.io.IOException

// The retired DataStore stand-in, kept only so a device that wrote settings
// before the core owned them carries those choices across the update. A
// corrupt or unreadable file falls back to empty, which reads as "nothing
// to migrate".
private val Context.legacySettingsStore by preferencesDataStore(
    name = "inkuna_settings",
    corruptionHandler = ReplaceFileCorruptionHandler { emptyPreferences() },
)

/**
 * App settings on the core's settings store, so preferences live beside the
 * library database (and sync with it, once sync exists).
 *
 * The shape mirrors iOS: one in-memory [Snapshot] the UI reads and writes
 * synchronously, loaded from the core once before anything composes, with
 * every change pushed back as a whole-record write on the application write
 * scope — a screen's disposal never cancels a settings write mid-flight,
 * and later writes simply persist the newest snapshot again.
 */
class AppSettings private constructor(private val context: Context) {

    data class Snapshot(
        val onboarded: Boolean = false,
        val readingTheme: ReadingTheme = ReadingTheme.Paper,
        val textSizeStep: Int = DEFAULT_TEXT_SIZE_STEP,
        val brightness: Float = DEFAULT_BRIGHTNESS,
    )

    private val _snapshot = MutableStateFlow(Snapshot())
    val snapshot: StateFlow<Snapshot> = _snapshot.asStateFlow()

    private val loadLock = Mutex()
    private var loaded = false

    // One write at a time: each persist stores the snapshot as it stands
    // when its turn comes, so the record that lands last is the newest.
    private val writeLock = Mutex()

    /**
     * Reads the stored settings into [snapshot], migrating the legacy
     * DataStore file on first encounter. Called before `setContent` on
     * every entry activity; after the first call it is a cheap no-op.
     *
     * A library that will not open falls back to defaults without marking
     * the load done — the next launch retries, and the screens that need
     * the shelf surface the failure themselves.
     */
    suspend fun load(): Snapshot = loadLock.withLock {
        if (loaded) return _snapshot.value
        runCatching {
            val bookshelf = LibraryStore.bookshelf(context)
            migrateLegacyStore(bookshelf.settings())
                ?.also { bookshelf.setSettings(it) }
                ?: bookshelf.settings()
        }.onSuccess { record ->
            _snapshot.value = record.toSnapshot()
            loaded = true
        }.onFailure {
            Log.w(TAG, "Settings would not load; running on defaults", it)
        }
        _snapshot.value
    }

    /**
     * The old DataStore values overlaid on the core's record, or null when
     * there is nothing to migrate. The file is cleared afterwards so the
     * migration happens exactly once.
     */
    private suspend fun migrateLegacyStore(current: Settings): Settings? {
        val prefs = context.legacySettingsStore.data
            .catch { error -> if (error is IOException) emit(emptyPreferences()) else throw error }
            .first()
        if (prefs.asMap().isEmpty()) return null
        val migrated = current.copy(
            onboarded = prefs[LegacyKeys.onboarded] ?: current.onboarded,
            readingTheme = prefs[LegacyKeys.readingTheme] ?: current.readingTheme,
            textSizeStep = prefs[LegacyKeys.textSizeStep]
                ?.coerceIn(0, TEXT_SIZE_STEPS.lastIndex)?.toUByte()
                ?: current.textSizeStep,
            brightness = prefs[LegacyKeys.brightness]
                ?.coerceIn(0f, 1f)?.toDouble()
                ?: current.brightness,
        )
        context.legacySettingsStore.edit { it.clear() }
        return migrated
    }

    fun setOnboarded(value: Boolean) =
        update { it.copy(onboarded = value) }

    fun setReadingTheme(theme: ReadingTheme) =
        update { it.copy(readingTheme = theme) }

    fun setTextSizeStep(step: Int) =
        update { it.copy(textSizeStep = step.coerceIn(0, TEXT_SIZE_STEPS.lastIndex)) }

    fun setBrightness(value: Float) =
        update { it.copy(brightness = value.coerceIn(0f, 1f)) }

    // The snapshot updates synchronously — the caller's screen renders the
    // change next frame — and persisting trails best-effort behind it.
    private fun update(transform: (Snapshot) -> Snapshot) {
        _snapshot.updateAndGet(transform)
        LibraryStore.writes.launch {
            writeLock.withLock {
                runCatching {
                    LibraryStore.bookshelf(context).setSettings(_snapshot.value.toRecord())
                }.onFailure { Log.w(TAG, "A settings write failed", it) }
            }
        }
    }

    private fun Settings.toSnapshot() = Snapshot(
        onboarded = onboarded,
        // The core treats the theme as an opaque string the shells own;
        // both shells write the lowercase names, and an unrecognized value
        // (a future theme, a hand-edited database) reads as Paper.
        readingTheme = ReadingTheme.entries
            .firstOrNull { it.name.equals(readingTheme, ignoreCase = true) }
            ?: ReadingTheme.Paper,
        textSizeStep = textSizeStep.toInt().coerceIn(0, TEXT_SIZE_STEPS.lastIndex),
        brightness = brightness.toFloat().coerceIn(0f, 1f),
    )

    private fun Snapshot.toRecord() = Settings(
        onboarded = onboarded,
        readingTheme = readingTheme.name.lowercase(),
        textSizeStep = textSizeStep.toUByte(),
        brightness = brightness.toDouble(),
    )

    private object LegacyKeys {
        val onboarded = booleanPreferencesKey("onboarded")
        val readingTheme = stringPreferencesKey("reading_theme")
        val textSizeStep = intPreferencesKey("text_size_step")
        val brightness = floatPreferencesKey("brightness")
    }

    companion object {
        /** Five reading sizes, the design's 0.9–1.25rem scale. */
        val TEXT_SIZE_STEPS = listOf(14.4f, 15.6f, 17f, 18.4f, 20f)
        const val DEFAULT_TEXT_SIZE_STEP = 2
        const val DEFAULT_BRIGHTNESS = 0.78f

        private const val TAG = "InkunaSettings"

        @Volatile
        private var instance: AppSettings? = null

        /** The process-wide settings, shared by every entry activity. */
        fun get(context: Context): AppSettings =
            instance ?: synchronized(this) {
                instance ?: AppSettings(context.applicationContext).also { instance = it }
            }
    }
}
