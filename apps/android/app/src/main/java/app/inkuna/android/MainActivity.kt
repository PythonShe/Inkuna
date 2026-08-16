package app.inkuna.android

import android.app.UiModeManager
import android.os.Bundle
import android.util.Log
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.fragment.app.FragmentActivity
import androidx.lifecycle.lifecycleScope
import app.inkuna.android.model.AppSettings
import app.inkuna.android.model.LibraryStore
import app.inkuna.android.ui.InkunaApp
import app.inkuna.android.ui.reader.READER_NAVIGATOR_FRAGMENT_TAG
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
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
        // Warm the core library off the main thread — opening runs schema
        // migrations and an orphaned-file sweep synchronously. Failure is
        // recoverable, not fatal: nothing is cached on error, so the first
        // screen that needs the shelf retries and surfaces it.
        lifecycleScope.launch {
            runCatching { LibraryStore.bookshelf(applicationContext) }
                .onFailure { Log.w("Inkuna", "core library warm-up failed", it) }
        }
        setContent {
            InkunaApp(settings = settings, initial = initial)
        }
    }
}
