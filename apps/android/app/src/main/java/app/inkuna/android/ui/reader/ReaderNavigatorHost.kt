package app.inkuna.android.ui.reader

import android.os.Build
import android.view.View
import androidx.activity.compose.LocalActivity
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.viewinterop.AndroidView
import androidx.fragment.app.FragmentActivity
import androidx.fragment.app.FragmentContainerView
import androidx.fragment.app.commitNow
import app.inkuna.android.R
import org.readium.r2.navigator.epub.EpubNavigatorFactory
import org.readium.r2.navigator.epub.EpubNavigatorFragment
import org.readium.r2.navigator.epub.EpubPreferences
import org.readium.r2.shared.ExperimentalReadiumApi
import org.readium.r2.shared.publication.Locator

/**
 * Tag of the hosted navigator fragment. `MainActivity` installs Readium's
 * dummy factory before `super.onCreate` and removes any fragment restored
 * under this tag: the real navigator can only be built once its publication
 * is open (async), so the reader re-adds a fresh one at the core's saved
 * locator instead of restoring stale fragment state.
 */
const val READER_NAVIGATOR_FRAGMENT_TAG = "reader-epub-navigator"

/**
 * Hosts Readium's fragment-based [EpubNavigatorFragment] inside Compose.
 * The fragment lives in the activity's FragmentManager inside a
 * [FragmentContainerView]; [onNavigator] hands the live navigator to the
 * caller and reports `null` again when the host leaves composition.
 */
@OptIn(ExperimentalReadiumApi::class)
@Composable
fun ReaderNavigatorHost(
    navigatorFactory: EpubNavigatorFactory,
    initialLocator: Locator?,
    initialPreferences: EpubPreferences,
    listener: EpubNavigatorFragment.Listener,
    styleInjector: ReaderStyleInjector,
    onPager: (ReaderPagerLayout?) -> Unit,
    onNavigator: (EpubNavigatorFragment?) -> Unit,
    modifier: Modifier = Modifier,
) {
    val activity = LocalActivity.current as FragmentActivity

    // Set by the AndroidView factory, which runs before the effect below.
    val pagerLayout = remember { mutableStateOf<ReaderPagerLayout?>(null) }
    AndroidView(
        modifier = modifier,
        factory = { context ->
            // The pager wraps the fragment container and owns every
            // horizontal page turn — Readium's own gesture pipeline never
            // sees a horizontal drag; see ReaderPagerLayout.
            ReaderPagerLayout(context).apply {
                addView(FragmentContainerView(context).apply { id = R.id.reader_navigator_host })
                // Frame-rate votes don't propagate to children; the tuner
                // covers the WebViews and pager, this covers the host.
                if (Build.VERSION.SDK_INT >= 35) {
                    requestedFrameRate = View.REQUESTED_FRAME_RATE_CATEGORY_HIGH
                }
                pagerLayout.value = this
                onPager(this)
            }
        },
    )

    // API 33/34 have no per-view frame-rate votes; ask the window for the
    // display's fastest same-resolution mode while the reader is open.
    // (On 35+ the view votes above are the mechanism, and this attribute
    // would be ignored anyway wherever Surface.setFrameRate is in play.)
    if (Build.VERSION.SDK_INT < 35) {
        DisposableEffect(Unit) {
            val window = activity.window
            val previous = window.attributes.preferredRefreshRate
            val mode = activity.display?.mode
            val fastest = activity.display?.supportedModes
                ?.filter {
                    it.physicalWidth == mode?.physicalWidth &&
                        it.physicalHeight == mode?.physicalHeight
                }
                ?.maxOfOrNull { it.refreshRate } ?: 0f
            if (fastest > 0f) {
                window.attributes = window.attributes.apply { preferredRefreshRate = fastest }
            }
            onDispose {
                window.attributes = window.attributes.apply { preferredRefreshRate = previous }
            }
        }
    }

    // Keyed on the factory: one fragment per opened publication. Theme and
    // type changes go through submitPreferences, never a rebuild.
    DisposableEffect(navigatorFactory) {
        val fragmentManager = activity.supportFragmentManager
        val fragment = navigatorFactory.createFragmentFactory(
            initialLocator = initialLocator,
            initialPreferences = initialPreferences,
            listener = listener,
            // The reading band is owned entirely by Compose (ReaderMetrics
            // via ReaderScreen): Readium's own cutout padding would stack
            // on top of it and double-count notched devices. Its hidden
            // 40dp paginated padding is zeroed in res/values/dimens.xml
            // for the same reason.
            configuration = EpubNavigatorFragment.Configuration(
                shouldApplyInsetsPadding = false,
                // The bundled Noto files, served from app assets on the
                // CORS-enabled https://readium_assets/ origin. Which family
                // the page uses is decided by our own injected stylesheet
                // (ReaderUserCss), never by an EpubPreferences value.
                servedAssets = listOf("fonts/.*"),
            ),
        ).instantiate(
            activity.classLoader,
            EpubNavigatorFragment::class.java.name,
        ) as EpubNavigatorFragment
        // commitNow, not commit: the navigator must exist before the first
        // frame that expects it, and the container view is already attached.
        fragmentManager.commitNow {
            setReorderingAllowed(true)
            replace(R.id.reader_navigator_host, fragment, READER_NAVIGATOR_FRAGMENT_TAG)
        }
        // Overscroll stretch off and high frame-rate votes on, for every
        // resource WebView the pager creates; see ReaderWebViewTuner.
        val webViewTuner = fragment.view?.let { ReaderWebViewTuner(it, styleInjector) }
        webViewTuner?.attach()
        pagerLayout.value?.navigator = fragment
        onNavigator(fragment)
        onDispose {
            pagerLayout.value?.let { layout ->
                layout.cancelInteraction()
                layout.navigator = null
            }
            onPager(null)
            webViewTuner?.detach()
            onNavigator(null)
            // After onSaveInstanceState the FragmentManager refuses
            // transactions; the activity is going down with its fragments
            // anyway, and MainActivity removes the restored dummy on the
            // way back up.
            if (!fragmentManager.isStateSaved) {
                fragmentManager.commitNow { remove(fragment) }
            }
        }
    }
}
