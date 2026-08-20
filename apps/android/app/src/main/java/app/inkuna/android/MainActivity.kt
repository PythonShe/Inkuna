package app.inkuna.android

import android.app.UiModeManager
import android.os.Bundle
import android.view.ActionMode
import android.webkit.WebView
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.fragment.app.FragmentActivity
import androidx.lifecycle.lifecycleScope
import app.inkuna.android.model.AppSettings
import app.inkuna.android.reminder.EveningReminder
import app.inkuna.android.ui.InkunaApp
import app.inkuna.android.ui.reader.READER_NAVIGATOR_FRAGMENT_TAG
import app.inkuna.android.ui.reader.SelectionModeTracker
import kotlinx.coroutines.launch
import org.readium.r2.navigator.epub.EpubNavigatorFragment

// A FragmentActivity because Readium's EPUB navigator is a Fragment; the
// rest of the shell stays pure Compose.
class MainActivity : FragmentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        // The navigator fragment can only be built once its publication is
        // open, which is async — so a recreated activity restores a dummy
        // (Readium's sanctioned pattern), removed below before anything
        // resumes; the reader then re-adds a real one at the core's saved
        // locator.
        supportFragmentManager.fragmentFactory = EpubNavigatorFragment.createDummyFactory()
        super.onCreate(savedInstanceState)
        supportFragmentManager.findFragmentByTag(READER_NAVIGATOR_FRAGMENT_TAG)?.let { restored ->
            supportFragmentManager.beginTransaction().remove(restored).commitNow()
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
            // Warm the WebView provider behind the first frame: the first
            // WebView a process creates pays for loading the provider and
            // spinning up the renderer, and unwarmed that bill lands inside
            // the reader's push animation instead of here, where nothing
            // is moving. The instance itself is discarded; only the
            // process-wide initialization is the point.
            // runCatching: a disabled, missing, or mid-update WebView
            // provider throws from the constructor, and a warm-up must
            // never be the reason the whole app fails to launch.
            window.decorView.post {
                if (!isDestroyed) {
                    runCatching { WebView(this@MainActivity).destroy() }
                }
            }
        }
    }

    // Text-selection tracking for the reader's pager: the WebView starts
    // its selection ActionMode through the activity, so these overrides
    // see every selection without owning the selection menu. While one is
    // live, horizontal drags belong to the handles, not to page turns.

    override fun onActionModeStarted(mode: ActionMode?) {
        super.onActionModeStarted(mode)
        SelectionModeTracker.started()
    }

    override fun onActionModeFinished(mode: ActionMode?) {
        super.onActionModeFinished(mode)
        SelectionModeTracker.finished()
    }
}
