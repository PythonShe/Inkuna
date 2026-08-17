package app.inkuna.android.ui

import android.app.Activity
import android.app.UiModeManager
import android.net.Uri
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalView
import androidx.core.view.WindowCompat
import androidx.lifecycle.Lifecycle
import androidx.navigation.NavHostController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import androidx.navigation.navArgument
import app.inkuna.android.model.AppSettings
import app.inkuna.android.ui.detail.BookDetailScreen
import app.inkuna.android.ui.main.MainScreen
import app.inkuna.android.ui.onboarding.ThemePickScreen
import app.inkuna.android.ui.onboarding.WelcomeScreen
import app.inkuna.android.ui.reader.ReaderScreen
import app.inkuna.android.ui.theme.InkMotion
import app.inkuna.android.ui.theme.InkTheme

private object Routes {
    const val WELCOME = "welcome"
    const val THEME_PICK = "themepick"
    const val MAIN = "main"

    /** Both book screens are core-addressed: the argument is a
     *  `Publication` id; the reader optionally takes a chapter href to
     *  open at instead of the saved position. */
    const val DETAIL = "detail/{publicationId}"
    const val READER = "reader/{publicationId}?chapter={chapter}"

    fun detail(publicationId: String) = "detail/${Uri.encode(publicationId)}"

    fun reader(publicationId: String, chapterHref: String? = null): String {
        val base = "reader/${Uri.encode(publicationId)}"
        return if (chapterHref == null) base else "$base?chapter=${Uri.encode(chapterHref)}"
    }
}

/**
 * Pushes [route] once. A second tap landing inside the 320ms page
 * transition finds the source entry no longer RESUMED and is dropped, so a
 * double-tap can't stack two copies of a screen.
 */
private fun NavHostController.pushOnce(route: String) {
    val entry = currentBackStackEntry ?: return
    if (!entry.lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)) return
    navigate(route) { launchSingleTop = true }
}

/** Opens the reader for a core publication, popping back to an existing
 *  reader instead of stacking a second one (mirrors the iOS review fix). */
private fun NavHostController.openReader(publicationId: String, chapterHref: String? = null) {
    val route = Routes.reader(publicationId, chapterHref)
    if (!popBackStack(route, inclusive = false)) {
        pushOnce(route)
    }
}

@Composable
fun InkunaApp(settings: AppSettings, initial: AppSettings.Snapshot) {
    val snapshot by settings.snapshot.collectAsState(initial)
    val night = snapshot.readingTheme.isNight

    // Keep the system bars and the per-app night qualifier (launch window
    // background) in step with the reading theme — the app never follows
    // system dark mode; the reading surface decides.
    val view = LocalView.current
    val context = LocalContext.current
    LaunchedEffect(night) {
        (view.context as? Activity)?.window?.let { window ->
            WindowCompat.getInsetsController(window, view).apply {
                isAppearanceLightStatusBars = !night
                isAppearanceLightNavigationBars = !night
            }
        }
        context.getSystemService(UiModeManager::class.java)?.setApplicationNightMode(
            if (night) UiModeManager.MODE_NIGHT_YES else UiModeManager.MODE_NIGHT_NO
        )
    }

    InkTheme(night = night) {
        val nav = rememberNavController()
        val pageEasing = InkMotion.easePage

        NavHost(
            navController = nav,
            startDestination = if (initial.onboarded) Routes.MAIN else Routes.WELCOME,
            modifier = Modifier
                .fillMaxSize()
                .background(InkTheme.colors.bgApp),
            enterTransition = {
                slideInHorizontally(tween(InkMotion.durMed, easing = pageEasing)) { it } +
                    fadeIn(tween(InkMotion.durMed))
            },
            exitTransition = {
                slideOutHorizontally(tween(InkMotion.durMed, easing = pageEasing)) { -it / 4 } +
                    fadeOut(tween(InkMotion.durMed))
            },
            popEnterTransition = {
                slideInHorizontally(tween(InkMotion.durMed, easing = pageEasing)) { -it / 4 } +
                    fadeIn(tween(InkMotion.durMed))
            },
            popExitTransition = {
                slideOutHorizontally(tween(InkMotion.durMed, easing = pageEasing)) { it } +
                    fadeOut(tween(InkMotion.durMed))
            },
        ) {
            composable(
                Routes.WELCOME,
                exitTransition = { fadeOut(tween(InkMotion.durMed)) },
            ) {
                WelcomeScreen(onBegin = { nav.pushOnce(Routes.THEME_PICK) })
            }
            composable(Routes.THEME_PICK) {
                ThemePickScreen(
                    selectedTheme = snapshot.readingTheme,
                    onPick = { theme -> settings.setReadingTheme(theme) },
                    onContinue = {
                        settings.setOnboarded(true)
                        nav.navigate(Routes.MAIN) {
                            popUpTo(Routes.WELCOME) { inclusive = true }
                            launchSingleTop = true
                        }
                    },
                )
            }
            composable(
                Routes.MAIN,
                // Onboarding hands off with a long cross-fade, not a push.
                enterTransition = { fadeIn(tween(InkMotion.durSlow)) },
            ) {
                MainScreen(
                    onOpenBook = { id -> nav.pushOnce(Routes.detail(id)) },
                    onOpenReader = { id -> nav.openReader(id) },
                )
            }
            composable(Routes.DETAIL) { entry ->
                val publicationId = entry.arguments?.getString("publicationId").orEmpty()
                BookDetailScreen(
                    publicationId = publicationId,
                    onBack = { nav.popBackStack() },
                    onRead = { chapterHref -> nav.openReader(publicationId, chapterHref) },
                )
            }
            composable(
                Routes.READER,
                arguments = listOf(navArgument("chapter") { nullable = true }),
                // The reader hosts a live WebView: fading it composites the
                // whole page through an animated alpha layer every frame, so
                // its push and pop slide without the fade. The pop rides the
                // system back gesture, which commits mid-slide — the default
                // sharp ease-out plays the remainder in a blink, so leaving
                // a book takes the slow duration on the quiet curve instead.
                enterTransition = {
                    slideInHorizontally(tween(InkMotion.durMed, easing = pageEasing)) { it }
                },
                popExitTransition = {
                    slideOutHorizontally(tween(InkMotion.durSlow, easing = InkMotion.easeQuiet)) { it }
                },
            ) { entry ->
                ReaderScreen(
                    publicationId = entry.arguments?.getString("publicationId").orEmpty(),
                    initialChapterHref = entry.arguments?.getString("chapter"),
                    settings = settings,
                    snapshot = snapshot,
                    onBack = { nav.popBackStack() },
                )
            }
        }
    }
}
