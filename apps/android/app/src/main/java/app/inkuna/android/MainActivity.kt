package app.inkuna.android

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
        val initial = runBlocking { settings.snapshot.first() }
        // TODO(core): open app.inkuna.core.Bookshelf here and feed the
        // library screens; PlaceholderLibrary stands in until then.
        setContent {
            InkunaApp(settings = settings, initial = initial)
        }
    }
}
