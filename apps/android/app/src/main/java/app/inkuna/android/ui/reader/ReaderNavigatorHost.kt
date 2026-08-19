package app.inkuna.android.ui.reader

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
    boundarySignal: BoundaryGestureSignal,
    onNavigator: (EpubNavigatorFragment?) -> Unit,
    modifier: Modifier = Modifier,
) {
    val activity = LocalActivity.current as FragmentActivity

    // Set by the AndroidView factory, which runs before the effect below.
    val follower = remember { mutableStateOf<BoundaryDragFollower?>(null) }
    AndroidView(
        modifier = modifier,
        factory = { context ->
            // The follower wraps the fragment container so chapter-boundary
            // drags can be intercepted and replayed into Readium's pager;
            // see BoundaryDragFollower.
            BoundaryDragFollower(context, boundarySignal).apply {
                addView(FragmentContainerView(context).apply { id = R.id.reader_navigator_host })
                follower.value = this
            }
        },
    )

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
        // Readium leaves the resource WebViews on Android's default
        // overscroll mode, which paints a stretch edge effect on boundary
        // drags; see WebViewStretchSuppressor.
        val stretchSuppressor = fragment.view?.let(::WebViewStretchSuppressor)
        stretchSuppressor?.attach()
        follower.value?.navigator = fragment
        onNavigator(fragment)
        onDispose {
            follower.value?.navigator = null
            stretchSuppressor?.detach()
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
