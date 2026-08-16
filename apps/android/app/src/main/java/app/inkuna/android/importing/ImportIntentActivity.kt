package app.inkuna.android.importing

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.lifecycleScope
import app.inkuna.android.MainActivity
import app.inkuna.android.model.AppSettings
import app.inkuna.android.ui.importing.ImportSheet
import app.inkuna.android.ui.theme.InkTheme
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch

/**
 * Handles an EPUB arriving from outside the app — tapped in Files, shared
 * from a mail client, opened from a browser download.
 *
 * It is a separate activity rather than a branch inside `MainActivity` for
 * two reasons: `MainActivity` is a `FragmentActivity` hosting the Readium
 * navigator and has its own lifecycle to protect, and an inbound import
 * should be able to run and get out of the way without disturbing whatever
 * the reader was doing. The window is a transparent scrim plus the ordinary
 * import sheet, so it reads as a system-level "adding this book".
 *
 * The URI grant that comes with the intent lives only as long as this
 * activity, which is why the file is copied into cache immediately (see
 * [ImportStaging]) instead of being handed to the core by reference. The
 * run itself belongs to [ImportEngine] on the application write scope, so
 * backing out of this window does not abandon a book mid-import.
 */
class ImportIntentActivity : ComponentActivity() {

    private var incoming by mutableStateOf<List<Uri>>(emptyList())

    /**
     * Bumped on every intent this activity is handed. It is what separates
     * "the composition restarted" (same delivery, must not import twice)
     * from "the same book was sent again" (a new delivery, must import) —
     * the URI alone cannot tell those apart.
     *
     * Saved across recreation: the counter is half of the key the saved
     * `startedFor` is compared against, so losing it on a rotation would
     * read as a fresh delivery and import the same book twice.
     */
    private var delivery by mutableIntStateOf(0)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        delivery = savedInstanceState?.getInt(STATE_DELIVERY, 0) ?: 0
        // The URIs are restored alongside the counter: `setIntent` never
        // reaches the relaunch record, so after a rotation `getIntent()`
        // replays the FIRST delivery — trusting it would re-import that one
        // under the newest delivery's number.
        incoming = savedInstanceState
            ?.getParcelableArrayList(STATE_URIS, Uri::class.java)
            ?: urisFrom(intent)
        if (incoming.isEmpty()) {
            finish()
            return
        }
        // The same load MainActivity does — off the main thread while the
        // (transparent) launch window holds — so the sheet opens on the
        // reader's chosen ground instead of flashing the wrong one.
        val settings = AppSettings.get(applicationContext)
        lifecycleScope.launch {
            settings.load()
            setContent {
                val snapshot by settings.snapshot.collectAsState()
                InkTheme(night = snapshot.readingTheme.isNight) {
                    Box(Modifier.fillMaxSize().background(InkTheme.colors.scrim)) {
                        InboundImport(
                            uris = incoming,
                            delivery = delivery,
                            onDone = { addedSomething ->
                                if (addedSomething) {
                                    // NEW_TASK so the library comes up in
                                    // Inkuna's own task rather than on top of
                                    // the caller's (Files, Gmail) back stack.
                                    // SINGLE_TOP because CLEAR_TOP alone would
                                    // finish and recreate a standard-launch
                                    // MainActivity — tearing down an open
                                    // reader — instead of onNewIntent-ing it.
                                    startActivity(
                                        Intent(this@ImportIntentActivity, MainActivity::class.java)
                                            .addFlags(
                                                Intent.FLAG_ACTIVITY_NEW_TASK or
                                                    Intent.FLAG_ACTIVITY_CLEAR_TOP or
                                                    Intent.FLAG_ACTIVITY_SINGLE_TOP
                                            )
                                    )
                                }
                                finish()
                            },
                        )
                    }
                }
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        // ComponentActivity does not do this for us, and a recreation
        // replays getIntent() — without it a rotation after a second book
        // arrived would re-run the *first* one.
        setIntent(intent)
        val next = urisFrom(intent)
        if (next.isNotEmpty()) {
            incoming = next
            delivery++
        }
    }

    override fun onSaveInstanceState(outState: Bundle) {
        super.onSaveInstanceState(outState)
        outState.putInt(STATE_DELIVERY, delivery)
        outState.putParcelableArrayList(STATE_URIS, ArrayList(incoming))
    }

    /**
     * Only `content:` URIs are accepted. `EXTRA_STREAM` is a plain extra —
     * StrictMode's file-URI check never sees it — so any installed app
     * could otherwise hand this exported activity a `file:///data/user/0/…`
     * path into our own sandbox and have it opened under our uid.
     */
    private fun urisFrom(intent: Intent?): List<Uri> = when (intent?.action) {
        Intent.ACTION_VIEW -> listOfNotNull(intent.data)
        Intent.ACTION_SEND -> listOfNotNull(intent.streamExtra())
        Intent.ACTION_SEND_MULTIPLE -> intent.streamExtras()
        else -> emptyList()
    }.filter { it.scheme == "content" }

    // minSdk 33 is Tiramisu, so the typed Parcelable getters are the only
    // ones needed — no deprecated fallback.
    private fun Intent.streamExtra(): Uri? = getParcelableExtra(Intent.EXTRA_STREAM, Uri::class.java)

    private fun Intent.streamExtras(): List<Uri> =
        getParcelableArrayListExtra(Intent.EXTRA_STREAM, Uri::class.java).orEmpty()

    private companion object {
        const val STATE_DELIVERY = "inkuna.import.delivery"
        const val STATE_URIS = "inkuna.import.uris"
    }
}

/**
 * Runs the inbound selection through the same engine, sheet, and reporting
 * the in-app flow uses — an EPUB from Files must not get a second, lesser
 * import path.
 */
@Composable
private fun InboundImport(
    uris: List<Uri>,
    delivery: Int,
    onDone: (addedSomething: Boolean) -> Unit,
) {
    val context = LocalContext.current
    val state by ImportEngine.state.collectAsStateWithLifecycle()

    // Keyed on the delivery *and* the selection, saved across recreation: a
    // rotation mid-import must not start the run a second time, while the
    // same book sent again — a new delivery — must.
    val selectionKey = "$delivery:" + uris.joinToString("|")
    var started by rememberSaveable(selectionKey) { mutableStateOf(false) }
    LaunchedEffect(selectionKey) {
        // An in-app run may already be live. Waiting for Idle and asking
        // again — instead of dropping the delivery — is what keeps this
        // window from presenting a stranger's report as this book's.
        while (!started) {
            ImportEngine.state.first { it is ImportState.Idle }
            started = ImportEngine.start(context, uris)
        }
    }

    val report = (state as? ImportState.Finished)?.report
    ImportSheet(
        state = state,
        onCancel = ImportEngine::cancel,
        onDismiss = {
            ImportEngine.dismiss()
            // Leaving is only right once OUR run's report was the one shown;
            // dismissing the earlier run's report merely frees the engine
            // for the waiting delivery above.
            if (started) onDone(report?.changedLibrary == true)
        },
    )
}
