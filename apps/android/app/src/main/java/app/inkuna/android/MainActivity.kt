package app.inkuna.android

import android.app.UiModeManager
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import app.inkuna.android.model.AppSettings
import app.inkuna.android.ui.InkunaApp
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        val settings = AppSettings(applicationContext)
        // One blocking read so the first frame renders the right theme and
        // start destination — the stand-in for iOS's synchronous defaults.
        // AppSettings.snapshot swallows IO/corruption, so this cannot throw.
        val initial = runBlocking { settings.snapshot.first() }
        // Stamp the per-app night qualifier before anything composes: on a
        // first run there is none yet, so the launch window would otherwise
        // follow system dark mode and flash the wrong ground.
        getSystemService(UiModeManager::class.java)?.setApplicationNightMode(
            if (initial.readingTheme.isNight) UiModeManager.MODE_NIGHT_YES
            else UiModeManager.MODE_NIGHT_NO
        )
        // TODO(core): open app.inkuna.core.Bookshelf here and feed the
        // library screens; PlaceholderLibrary stands in until then.
        setContent {
            InkunaApp(settings = settings, initial = initial)
        }
    }
}
