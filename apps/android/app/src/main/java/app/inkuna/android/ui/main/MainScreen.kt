package app.inkuna.android.ui.main

import androidx.annotation.StringRes
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.LibraryBooks
import androidx.compose.material.icons.automirrored.outlined.LibraryBooks
import androidx.compose.material.icons.filled.AutoStories
import androidx.compose.material.icons.filled.BarChart
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.outlined.AutoStories
import androidx.compose.material.icons.outlined.BarChart
import androidx.compose.material.icons.outlined.Search
import androidx.compose.material3.Icon
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationBarItemDefaults
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.saveable.rememberSaveableStateHolder
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.stringResource
import app.inkuna.android.R
import app.inkuna.android.model.PlaceholderBook
import app.inkuna.android.ui.library.LibraryScreen
import app.inkuna.core.Publication as CorePublication
import app.inkuna.android.ui.search.SearchScreen
import app.inkuna.android.ui.stats.StatsScreen
import app.inkuna.android.ui.theme.InkTheme
import app.inkuna.android.ui.theme.InkType
import app.inkuna.android.ui.tonight.TonightScreen

/** The four shelf destinations, addressed by identity like iOS's MainTab. */
enum class MainTab(
    @param:StringRes val labelRes: Int,
    val icon: ImageVector,
    val selectedIcon: ImageVector,
) {
    Tonight(R.string.tab_tonight, Icons.Outlined.AutoStories, Icons.Filled.AutoStories),
    Library(
        R.string.tab_library,
        Icons.AutoMirrored.Outlined.LibraryBooks,
        Icons.AutoMirrored.Filled.LibraryBooks,
    ),
    Search(R.string.tab_search, Icons.Outlined.Search, Icons.Filled.Search),
    Stats(R.string.tab_stats, Icons.Outlined.BarChart, Icons.Filled.BarChart),
}

/**
 * The tab scaffold. Uses the native Material 3 navigation bar — the same
 * call iOS made in adopting the system tab bar over the prototype's
 * floating capsule.
 */
@Composable
fun MainScreen(
    onOpenBook: (PlaceholderBook) -> Unit,
    onOpenPublication: (String) -> Unit,
    continueReading: CorePublication?,
    onOpenReader: (CorePublication) -> Unit,
) {
    val ink = InkTheme.colors
    var selectedTab by rememberSaveable { mutableStateOf(MainTab.Tonight) }
    val stateHolder = rememberSaveableStateHolder()

    Scaffold(
        containerColor = ink.bgApp,
        // Scaffold's default insets are the system bars only; in landscape on
        // a notched device the page gutter alone doesn't clear the cutout.
        contentWindowInsets = WindowInsets.safeDrawing,
        bottomBar = {
            NavigationBar(containerColor = ink.bgSurface) {
                MainTab.entries.forEach { tab ->
                    val selected = tab == selectedTab
                    NavigationBarItem(
                        selected = selected,
                        onClick = { selectedTab = tab },
                        icon = {
                            Icon(
                                if (selected) tab.selectedIcon else tab.icon,
                                contentDescription = null,
                            )
                        },
                        label = { Text(stringResource(tab.labelRes), style = InkType.label) },
                        colors = NavigationBarItemDefaults.colors(
                            selectedIconColor = ink.accentText,
                            selectedTextColor = ink.accentText,
                            indicatorColor = ink.accentSoft,
                            unselectedIconColor = ink.textSecondary,
                            unselectedTextColor = ink.textSecondary,
                        ),
                    )
                }
            }
        },
    ) { innerPadding ->
        stateHolder.SaveableStateProvider(selectedTab.name) {
            when (selectedTab) {
                MainTab.Tonight -> TonightScreen(
                    innerPadding,
                    onOpenBook,
                    continueReading,
                    onOpenReader,
                )
                MainTab.Library -> LibraryScreen(innerPadding, onOpenPublication)
                MainTab.Search -> SearchScreen(innerPadding, onOpenBook)
                MainTab.Stats -> StatsScreen(innerPadding)
            }
        }
    }
}
