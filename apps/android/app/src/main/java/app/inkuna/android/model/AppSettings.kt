package app.inkuna.android.model

import android.content.Context
import androidx.datastore.core.handlers.ReplaceFileCorruptionHandler
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.emptyPreferences
import androidx.datastore.preferences.core.floatPreferencesKey
import androidx.datastore.preferences.core.intPreferencesKey
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import app.inkuna.android.ui.theme.ReadingTheme
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.map
import java.io.IOException

// A store truncated by a kill mid-write must never brick the app: a corrupt
// or unreadable file falls back to defaults instead of throwing out of the
// launch path.
private val Context.settingsStore by preferencesDataStore(
    name = "inkuna_settings",
    corruptionHandler = ReplaceFileCorruptionHandler { emptyPreferences() },
)

/**
 * App settings stand-in over DataStore.
 * TODO(core): persistence moves into the Rust core's settings store once it
 * lands, so preferences sync with the library database.
 */
class AppSettings(private val context: Context) {
    private object Keys {
        val onboarded = booleanPreferencesKey("onboarded")
        val readingTheme = stringPreferencesKey("reading_theme")
        val textSizeStep = intPreferencesKey("text_size_step")
        val brightness = floatPreferencesKey("brightness")
    }

    data class Snapshot(
        val onboarded: Boolean,
        val readingTheme: ReadingTheme,
        val textSizeStep: Int,
        val brightness: Float,
    )

    val snapshot: Flow<Snapshot> = context.settingsStore.data.catch { error ->
        if (error is IOException) emit(emptyPreferences()) else throw error
    }.map { prefs ->
        Snapshot(
            onboarded = prefs[Keys.onboarded] ?: false,
            readingTheme = prefs[Keys.readingTheme]
                ?.let { raw -> ReadingTheme.entries.firstOrNull { it.name.equals(raw, ignoreCase = true) } }
                ?: ReadingTheme.Paper,
            textSizeStep = (prefs[Keys.textSizeStep] ?: DEFAULT_TEXT_SIZE_STEP).coerceIn(0, TEXT_SIZE_STEPS.lastIndex),
            brightness = (prefs[Keys.brightness] ?: DEFAULT_BRIGHTNESS).coerceIn(0f, 1f),
        )
    }

    suspend fun setOnboarded(value: Boolean) {
        context.settingsStore.edit { it[Keys.onboarded] = value }
    }

    suspend fun setReadingTheme(theme: ReadingTheme) {
        context.settingsStore.edit { it[Keys.readingTheme] = theme.name }
    }

    suspend fun setTextSizeStep(step: Int) {
        context.settingsStore.edit { it[Keys.textSizeStep] = step.coerceIn(0, TEXT_SIZE_STEPS.lastIndex) }
    }

    suspend fun setBrightness(value: Float) {
        context.settingsStore.edit { it[Keys.brightness] = value.coerceIn(0f, 1f) }
    }

    companion object {
        /** Five reading sizes, the design's 0.9–1.25rem scale. */
        val TEXT_SIZE_STEPS = listOf(14.4f, 15.6f, 17f, 18.4f, 20f)
        const val DEFAULT_TEXT_SIZE_STEP = 2
        const val DEFAULT_BRIGHTNESS = 0.78f
    }
}
