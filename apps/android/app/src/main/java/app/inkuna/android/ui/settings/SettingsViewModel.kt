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
import app.inkuna.android.update.UpdateInstaller
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

        /** [apkUrl] enables the in-app install; null falls back to [url]. */
        data class Available(
            val versionName: String,
            val url: String,
            val apkUrl: String?,
        ) : UpdateState

        data class Downloading(val percent: Int) : UpdateState
        data object Installing : UpdateState
        data object Failed : UpdateState
        data object DownloadFailed : UpdateState
        data object InstallFailed : UpdateState
    }

    private val _updateState = MutableStateFlow<UpdateState>(UpdateState.Idle)
    val updateState: StateFlow<UpdateState> = _updateState.asStateFlow()

    init {
        // Install-session outcomes arrive through the manifest receiver.
        // Success never lands here — the update replaces the process.
        viewModelScope.launch {
            UpdateInstaller.events.collect { event ->
                when (event) {
                    // Backing out of the confirmation isn't a failure:
                    // re-offer the update instead of showing an error.
                    UpdateInstaller.Event.Aborted ->
                        _updateState.value = lastAvailable ?: UpdateState.Idle
                    UpdateInstaller.Event.Failed ->
                        _updateState.value = UpdateState.InstallFailed
                }
            }
        }
    }

    /** The verdict to fall back to when an install attempt is aborted. */
    private var lastAvailable: UpdateState.Available? = null

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
        when (_updateState.value) {
            // In-flight work writes its own outcome; leave it visible.
            UpdateState.Checking,
            is UpdateState.Downloading,
            UpdateState.Installing,
            -> Unit
            else -> _updateState.value = UpdateState.Idle
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
                            UpdateState.Available(
                                result.versionName,
                                result.url,
                                result.apkUrl,
                            ).also { lastAvailable = it }
                        UpdateCheck.UpToDate -> UpdateState.UpToDate
                    }
                },
                onFailure = { UpdateState.Failed },
            )
        }
    }

    /**
     * Downloads the offered APK and commits it to a PackageInstaller
     * session; the system takes over from there (confirmation dialog,
     * then the swap). Runs in the activity-scoped viewModelScope, so
     * closing the sheet doesn't cancel it — only leaving the app does.
     */
    fun downloadAndInstall() {
        val offer = _updateState.value as? UpdateState.Available ?: return
        val apkUrl = offer.apkUrl ?: return
        _updateState.value = UpdateState.Downloading(0)
        viewModelScope.launch {
            val apk = runCatching {
                withContext(Dispatchers.IO) {
                    UpdateInstaller.download(getApplication(), apkUrl) { percent ->
                        _updateState.value = UpdateState.Downloading(percent)
                    }
                }
            }.getOrElse {
                _updateState.value = UpdateState.DownloadFailed
                return@launch
            }
            _updateState.value = UpdateState.Installing
            runCatching {
                withContext(Dispatchers.IO) {
                    UpdateInstaller.install(getApplication(), apk)
                }
            }.onFailure { _updateState.value = UpdateState.InstallFailed }
        }
    }
}
