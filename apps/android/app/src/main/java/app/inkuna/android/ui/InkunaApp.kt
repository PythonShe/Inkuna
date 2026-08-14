package app.inkuna.android.ui

import android.app.Activity
import android.app.UiModeManager
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalView
import androidx.core.view.WindowCompat
import androidx.navigation.NavHostController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import app.inkuna.android.model.AppSettings
import app.inkuna.android.model.PlaceholderLibrary
import app.inkuna.android.ui.detail.BookDetailScreen
import app.inkuna.android.ui.main.MainScreen
import app.inkuna.android.ui.onboarding.ThemePickScreen
import app.inkuna.android.ui.onboarding.WelcomeScreen
import app.inkuna.android.ui.reader.ReaderScreen
import app.inkuna.android.ui.theme.InkMotion
import app.inkuna.android.ui.theme.InkTheme
import kotlinx.coroutines.launch

private object Routes {
    const val WELCOME = "welcome"
    const val THEME_PICK = "themepick"
    const val MAIN = "main"
    const val DETAIL = "detail/{bookId}"
    const val READER = "reader/{bookId}"

    fun detail(bookId: Int) = "detail/$bookId"
    fun reader(bookId: Int) = "reader/$bookId"
}

private fun bookFrom(route: androidx.navigation.NavBackStackEntry) =
    PlaceholderLibrary.books.firstOrNull {
        it.id == route.arguments?.getString("bookId")?.toIntOrNull()
    } ?: PlaceholderLibrary.heroBook

/** Opens the reader for [bookId], popping back to an existing reader
 *  instead of stacking a second one (mirrors the iOS review fix). */
private fun NavHostController.openReader(bookId: Int) {
    if (!popBackStack(Routes.reader(bookId), inclusive = false)) {
        navigate(Routes.reader(bookId))
    }
}

@Composable
fun InkunaApp(settings: AppSettings, initial: AppSettings.Snapshot) {
    val snapshot by settings.snapshot.collectAsState(initial)
    val scope = rememberCoroutineScope()
    val night = snapshot.readingTheme.isNight

    // Keep the system bars and the per-app night qualifier (launch window
    // background) in step with the reading theme — the app never follows
    // system dark mode; the reading surface decides.
    val view = LocalView.current
    val context = LocalContext.current
    androidx.compose.runtime.LaunchedEffect(night) {
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
                WelcomeScreen(onBegin = { nav.navigate(Routes.THEME_PICK) })
            }
            composable(Routes.THEME_PICK) {
                ThemePickScreen(
                    selectedTheme = snapshot.readingTheme,
                    onPick = { theme -> scope.launch { settings.setReadingTheme(theme) } },
                    onContinue = {
                        scope.launch { settings.setOnboarded(true) }
                        nav.navigate(Routes.MAIN) {
                            popUpTo(Routes.WELCOME) { inclusive = true }
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
                    onOpenBook = { book -> nav.navigate(Routes.detail(book.id)) },
                    onOpenReader = { book -> nav.openReader(book.id) },
                )
            }
            composable(Routes.DETAIL) { entry ->
                val book = bookFrom(entry)
                BookDetailScreen(
                    book = book,
                    onBack = { nav.popBackStack() },
                    onRead = { nav.openReader(book.id) },
                )
            }
            composable(Routes.READER) { entry ->
                ReaderScreen(
                    book = bookFrom(entry),
                    settings = settings,
                    snapshot = snapshot,
                    onBack = { nav.popBackStack() },
                )
            }
        }
    }
}
