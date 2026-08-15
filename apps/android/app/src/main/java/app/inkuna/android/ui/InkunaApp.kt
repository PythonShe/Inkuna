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
import androidx.lifecycle.Lifecycle
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.navigation.NavHostController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import app.inkuna.android.model.AppSettings
import app.inkuna.android.model.LibraryStore
import app.inkuna.android.model.PlaceholderLibrary
import app.inkuna.core.Publication as CorePublication
import app.inkuna.core.Shelf
import app.inkuna.core.Sort
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

    /** The reader is core-addressed: the argument is a `Publication` id. */
    const val READER = "reader/{publicationId}"

    fun detail(bookId: Int) = "detail/$bookId"
    fun reader(publicationId: String) = "reader/$publicationId"
}

private fun bookFrom(route: androidx.navigation.NavBackStackEntry) =
    PlaceholderLibrary.books.firstOrNull {
        it.id == route.arguments?.getString("bookId")?.toIntOrNull()
    } ?: PlaceholderLibrary.heroBook

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
private fun NavHostController.openReader(publication: CorePublication) {
    val route = Routes.reader(publication.id)
    if (!popBackStack(route, inclusive = false)) {
        pushOnce(route)
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

        // The reader is core-addressed, but the shelves still render
        // PlaceholderLibrary rows — so every "keep reading" affordance
        // carries the most recently opened core publication instead.
        // Refreshed on every navigation so a finished sitting moves the
        // hero. TODO(core): dissolve once the shelves themselves run on
        // Bookshelf queries and each row carries its own publication.
        var continueReading by remember { mutableStateOf<CorePublication?>(null) }
        val navEntry by nav.currentBackStackEntryAsState()
        LaunchedEffect(navEntry) {
            continueReading = runCatching {
                LibraryStore.bookshelf(context)
                    .list(Shelf.ALL, Sort.RECENTLY_OPENED)
                    .firstOrNull()
            }.getOrNull() ?: continueReading
        }
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
                    onPick = { theme -> scope.launch { settings.setReadingTheme(theme) } },
                    onContinue = {
                        scope.launch { settings.setOnboarded(true) }
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
                    onOpenBook = { book -> nav.pushOnce(Routes.detail(book.id)) },
                    continueReading = continueReading,
                    onOpenReader = { publication -> nav.openReader(publication) },
                )
            }
            composable(Routes.DETAIL) { entry ->
                BookDetailScreen(
                    book = bookFrom(entry),
                    publication = continueReading,
                    onBack = { nav.popBackStack() },
                    onRead = { publication -> nav.openReader(publication) },
                )
            }
            composable(Routes.READER) { entry ->
                ReaderScreen(
                    publicationId = entry.arguments?.getString("publicationId").orEmpty(),
                    settings = settings,
                    snapshot = snapshot,
                    onBack = { nav.popBackStack() },
                )
            }
        }
    }
}
