package app.inkuna.android.ui.settings

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import app.inkuna.android.BuildConfig
import app.inkuna.android.model.AppSettings
import app.inkuna.android.reminder.EveningReminder
import app.inkuna.android.ui.theme.ReadingTheme
import app.inkuna.android.update.UpdateCheck
import app.inkuna.android.update.UpdateChecker
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** The Account sheet's state and actions: settings writes, the reminder's
 *  scheduling side effect, and the Android-only update check. */
class SettingsViewModel(application: Application) : AndroidViewModel(application) {
    private val settings = AppSettings.get(application)

    val snapshot: StateFlow<AppSettings.Snapshot> = settings.snapshot

    sealed interface UpdateState {
        data object Idle : UpdateState
        data object Checking : UpdateState
        data object UpToDate : UpdateState
        data class Available(val versionName: String, val url: String) : UpdateState
        data object Failed : UpdateState
    }

    private val _updateState = MutableStateFlow<UpdateState>(UpdateState.Idle)
    val updateState: StateFlow<UpdateState> = _updateState.asStateFlow()

    private var nightFlip: Job? = null

    /** The night switch flips between the canonical day/night pair from
     *  onboarding, exactly like the design's toggle. The commit waits for
     *  the switch's thumb to land (a whole-app theme change mid-animation
     *  reads as a blink), and lives here rather than in the composable so
     *  dismissing the sheet inside the delay cannot cancel the write. */
    fun setNightModeDeferred(on: Boolean) {
        nightFlip?.cancel()
        nightFlip = viewModelScope.launch {
            delay(250)
            settings.setReadingTheme(if (on) ReadingTheme.Moon else ReadingTheme.Paper)
        }
    }

    /** Call only with notification permission in hand when [on]; the
     *  sheet owns the permission request. */
    fun setEveningReminder(on: Boolean) {
        settings.setEveningReminder(on)
        if (on) {
            EveningReminder.schedule(getApplication())
        } else {
            EveningReminder.cancel(getApplication())
        }
    }

    fun setAccount(name: String, email: String) = settings.setAccount(name, email)

    /** The VM outlives the sheet (activity-scoped), so without this a
     *  reopened sheet would still show last week's check verdict. A check
     *  still in flight is left alone — it writes its own outcome. */
    fun resetUpdateState() {
        if (_updateState.value != UpdateState.Checking) {
            _updateState.value = UpdateState.Idle
        }
    }

    fun checkForUpdate() {
        if (_updateState.value == UpdateState.Checking) return
        _updateState.value = UpdateState.Checking
        viewModelScope.launch {
            _updateState.value = runCatching {
                withContext(Dispatchers.IO) {
                    UpdateChecker.check(BuildConfig.VERSION_CODE.toLong())
                }
            }.fold(
                onSuccess = { result ->
                    when (result) {
                        is UpdateCheck.Available ->
                            UpdateState.Available(result.versionName, result.url)
                        UpdateCheck.UpToDate -> UpdateState.UpToDate
                    }
                },
                onFailure = { UpdateState.Failed },
            )
        }
    }
}
