package app.inkuna.android

import android.app.UiModeManager
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.lifecycle.lifecycleScope
import app.inkuna.android.debug.ParityDigestRunner
import app.inkuna.android.model.AppSettings
import app.inkuna.android.reminder.EveningReminder
import app.inkuna.android.ui.InkunaApp
import kotlinx.coroutines.launch
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (BuildConfig.DEBUG && intent.getBooleanExtra("inkuna.parityDigest", false)) {
            lifecycleScope.launch {
                ParityDigestRunner.run(applicationContext)
            }
            return
        }
        enableEdgeToEdge()
        val settings = AppSettings.get(applicationContext)
        // Settings live in the core's database now, and opening it runs
        // schema migrations and an orphaned-file sweep synchronously — so
        // the load happens off the main thread while the launch window
        // holds, the same way iOS holds its launch screen. The first frame
        // then renders the right theme and start destination, and the core
        // library is already warm. Load failure is recoverable, not fatal:
        // it falls back to defaults, and the first screen that needs the
        // shelf retries and surfaces the error.
        lifecycleScope.launch {
            val initial = settings.load()
            // Stamp the per-app night qualifier before anything composes:
            // on a first run there is none yet, so the launch window would
            // otherwise follow system dark mode and flash the wrong ground.
            getSystemService(UiModeManager::class.java)?.setApplicationNightMode(
                if (initial.readingTheme.isNight) UiModeManager.MODE_NIGHT_YES
                else UiModeManager.MODE_NIGHT_NO
            )
            setContent {
                InkunaApp(settings = settings, initial = initial)
            }
            // Re-anchor the pending reminder to the current timezone: the
            // enqueued delay is elapsed time, so a zone change while the app
            // slept would fire the nudge at the old zone's 21:00. REPLACE
            // policy makes this idempotent.
            if (initial.eveningReminder) {
                EveningReminder.schedule(applicationContext, initial.reminderMinutes)
            }
        }
    }
}
