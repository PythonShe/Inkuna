package app.inkuna.android.ui.settings

import android.Manifest
import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.ChevronRight
import androidx.compose.material.icons.outlined.DarkMode
import androidx.compose.material.icons.outlined.Info
import androidx.compose.material.icons.outlined.ManageAccounts
import androidx.compose.material.icons.outlined.Notifications
import androidx.compose.material.icons.outlined.Person
import androidx.compose.material.icons.outlined.SystemUpdate
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.core.net.toUri
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.launch
import app.inkuna.android.BuildConfig
import app.inkuna.android.R
import app.inkuna.android.ui.components.inkShadow
import app.inkuna.android.ui.main.EyebrowText
import app.inkuna.android.ui.reader.SheetCloseButton
import app.inkuna.android.ui.stats.hairlineThickness
import app.inkuna.android.ui.theme.InkRadius
import app.inkuna.android.ui.theme.InkSpace
import app.inkuna.android.ui.theme.InkTheme
import app.inkuna.android.ui.theme.InkType
import app.inkuna.android.ui.theme.ReadingTheme

/**
 * The Account sheet — the design's newest settings page: the purely local
 * profile, preferences (evening reminder, night mode), account editing,
 * the Android-only update check, and About.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsSheet(
    onDismiss: () -> Unit,
    model: SettingsViewModel = viewModel(),
) {
    val ink = InkTheme.colors
    val context = LocalContext.current
    val snapshot by model.snapshot.collectAsStateWithLifecycle()
    val updateState by model.updateState.collectAsStateWithLifecycle()
    var editingAccount by rememberSaveable { mutableStateOf(false) }

    // Turning the reminder on needs notification permission first; a
    // declined request leaves the switch (and the stored choice) off.
    val permissionLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission(),
    ) { granted -> if (granted) model.setEveningReminder(true) }

    // Explicit dismissals must animate the sheet away first, or it snaps.
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    val scope = rememberCoroutineScope()

    // The switch's optimistic position while the deferred flip (owned by
    // the ViewModel, so it survives the sheet's dismissal) is in flight;
    // cleared once the committed theme lands.
    var pendingNight by remember { mutableStateOf<Boolean?>(null) }
    LaunchedEffect(snapshot.readingTheme) { pendingNight = null }

    // The VM outlives the sheet; every opening starts from a fresh
    // "current version" line instead of a stale check verdict.
    LaunchedEffect(Unit) { model.resetUpdateState() }

    val animatedDismiss: () -> Unit = {
        scope.launch { sheetState.hide() }.invokeOnCompletion { onDismiss() }
    }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        containerColor = ink.bgApp,
        contentColor = ink.textBody,
        scrimColor = ink.scrim,
    ) {
        Column(
            Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = InkSpace.pageMargin)
                .padding(bottom = InkSpace.s10)
                .navigationBarsPadding(),
        ) {
            Row(
                Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    stringResource(R.string.settings_title),
                    style = InkType.title,
                    color = ink.textDisplay,
                    modifier = Modifier.weight(1f),
                )
                SheetCloseButton(onClick = animatedDismiss)
            }
            Spacer(Modifier.height(InkSpace.s5))

            ProfileCard(
                name = snapshot.accountName,
                email = snapshot.accountEmail,
            )
            Spacer(Modifier.height(InkSpace.s6))

            EyebrowText(stringResource(R.string.settings_preferences))
            Spacer(Modifier.height(10.dp))
            GroupCard {
                ToggleRow(
                    icon = { Icon(Icons.Outlined.Notifications, null, tint = ink.textSecondary) },
                    title = stringResource(R.string.settings_reminder),
                    subtitle = stringResource(R.string.settings_reminder_sub),
                    checked = snapshot.eveningReminder,
                    onCheckedChange = { on ->
                        if (!on) {
                            model.setEveningReminder(false)
                        } else if (
                            context.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) ==
                            PackageManager.PERMISSION_GRANTED
                        ) {
                            model.setEveningReminder(true)
                        } else {
                            permissionLauncher.launch(Manifest.permission.POST_NOTIFICATIONS)
                        }
                    },
                )
                Hairline()
                // The flip waits for the thumb to land, like iOS: the
                // whole-app theme change mid-animation reads as a blink.
                // The switch shows the pending value meanwhile — it is
                // state-driven, so without the override it wouldn't move
                // until the deferred commit lands.
                ToggleRow(
                    icon = { Icon(Icons.Outlined.DarkMode, null, tint = ink.textSecondary) },
                    title = stringResource(R.string.settings_night),
                    subtitle = stringResource(ReadingTheme.Moon.subtitleRes),
                    checked = pendingNight ?: snapshot.readingTheme.isNight,
                    onCheckedChange = { on ->
                        pendingNight = on
                        model.setNightModeDeferred(on)
                    },
                )
                // TODO(accounts): the design's "Redeem a code" row needs a
                // redemption backend; the account is purely local today.
            }
            Spacer(Modifier.height(InkSpace.s6))

            GroupCard {
                DisclosureRow(
                    icon = { Icon(Icons.Outlined.ManageAccounts, null, tint = ink.textSecondary) },
                    title = stringResource(R.string.settings_account_settings),
                    onClick = { editingAccount = true },
                )
                // TODO(accounts): the design's "Sign out" row waits on real
                // accounts; there is nothing to sign out of on a local one.
            }
            Spacer(Modifier.height(InkSpace.s6))

            GroupCard {
                UpdateRow(
                    state = updateState,
                    onCheck = model::checkForUpdate,
                    onGet = { url -> openWebPage(context, url) },
                )
                Hairline()
                DisclosureRow(
                    icon = { Icon(Icons.Outlined.Info, null, tint = ink.textSecondary) },
                    title = stringResource(R.string.settings_about),
                    onClick = { openWebPage(context, "https://inkuna.app") },
                )
            }
            Spacer(Modifier.height(InkSpace.s8))

            Text(
                stringResource(R.string.settings_footer, BuildConfig.VERSION_NAME),
                style = InkType.caption,
                color = ink.textTertiary,
                textAlign = TextAlign.Center,
                modifier = Modifier.fillMaxWidth(),
            )
        }
    }

    if (editingAccount) {
        AccountEditDialog(
            initialName = snapshot.accountName,
            initialEmail = snapshot.accountEmail,
            onSave = { name, email ->
                model.setAccount(name, email)
                editingAccount = false
            },
            onDismiss = { editingAccount = false },
        )
    }
}

/** Avatar plus name/email; empty fields render the local-account stand-ins. */
@Composable
private fun ProfileCard(name: String, email: String) {
    val ink = InkTheme.colors
    Row(
        Modifier
            .fillMaxWidth()
            .inkShadow(4.dp, InkRadius.lgShape)
            .clip(InkRadius.lgShape)
            .background(ink.bgSurface)
            .padding(InkSpace.s4),
        horizontalArrangement = Arrangement.spacedBy(InkSpace.s4),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            Modifier
                .size(52.dp)
                .clip(CircleShape)
                .background(ink.bgRecessed),
            contentAlignment = Alignment.Center,
        ) {
            val initial = name.firstOrNull()
            if (initial != null) {
                Text(
                    initial.uppercase(),
                    style = InkType.heading,
                    color = ink.textSecondary,
                )
            } else {
                Icon(Icons.Outlined.Person, null, tint = ink.textTertiary)
            }
        }
        Column {
            Text(
                name.ifEmpty { stringResource(R.string.settings_default_name) },
                style = InkType.heading,
                color = ink.textDisplay,
            )
            Text(
                email.ifEmpty { stringResource(R.string.settings_local_account) },
                style = InkType.caption,
                color = ink.textTertiary,
                modifier = Modifier.padding(top = 2.dp),
            )
        }
    }
}

/** The App updates row: current version or check outcome, and its action. */
@Composable
private fun UpdateRow(
    state: SettingsViewModel.UpdateState,
    onCheck: () -> Unit,
    onGet: (String) -> Unit,
) {
    val ink = InkTheme.colors
    val subtitle = when (state) {
        SettingsViewModel.UpdateState.Idle ->
            stringResource(R.string.settings_update_current, BuildConfig.VERSION_NAME)
        SettingsViewModel.UpdateState.Checking ->
            stringResource(R.string.settings_update_checking)
        SettingsViewModel.UpdateState.UpToDate ->
            stringResource(R.string.settings_update_uptodate)
        is SettingsViewModel.UpdateState.Available ->
            stringResource(R.string.settings_update_available, state.versionName)
        SettingsViewModel.UpdateState.Failed ->
            stringResource(R.string.settings_update_failed)
    }
    Row(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = InkSpace.s4, vertical = 14.dp),
        horizontalArrangement = Arrangement.spacedBy(InkSpace.s4),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(Icons.Outlined.SystemUpdate, null, tint = ink.textSecondary)
        Column(Modifier.weight(1f)) {
            Text(
                stringResource(R.string.settings_updates),
                style = InkType.ui,
                color = ink.textDisplay,
            )
            Text(
                subtitle,
                style = InkType.caption,
                color = ink.textTertiary,
                modifier = Modifier.padding(top = 2.dp),
            )
        }
        // TODO(updates): download and install the APK in-app; for now the
        // action opens the GitHub release page in the browser.
        when (state) {
            is SettingsViewModel.UpdateState.Available -> PillAction(
                text = stringResource(R.string.settings_update_get),
                onClick = { onGet(state.url) },
            )
            // The spinner holds the pill's slot so the row doesn't reflow
            // while the check (up to the 10s+10s timeouts) is in flight.
            SettingsViewModel.UpdateState.Checking -> CircularProgressIndicator(
                color = ink.accentText,
                strokeWidth = 2.dp,
                modifier = Modifier.size(18.dp),
            )
            else -> PillAction(
                text = stringResource(R.string.settings_update_check),
                onClick = onCheck,
            )
        }
    }
}

/** The design's small accent-soft pill button. */
@Composable
private fun PillAction(text: String, onClick: () -> Unit) {
    val ink = InkTheme.colors
    Text(
        text,
        style = InkType.caption,
        color = ink.accentText,
        modifier = Modifier
            .clip(InkRadius.pillShape)
            .background(ink.accentSoft)
            .clickable(onClick = onClick, role = Role.Button)
            .padding(horizontal = 14.dp, vertical = 7.dp),
    )
}

@Composable
private fun AccountEditDialog(
    initialName: String,
    initialEmail: String,
    onSave: (String, String) -> Unit,
    onDismiss: () -> Unit,
) {
    val ink = InkTheme.colors
    var name by rememberSaveable { mutableStateOf(initialName) }
    var email by rememberSaveable { mutableStateOf(initialEmail) }
    AlertDialog(
        onDismissRequest = onDismiss,
        containerColor = ink.bgSurface,
        titleContentColor = ink.textDisplay,
        title = { Text(stringResource(R.string.settings_account_settings), style = InkType.heading) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(InkSpace.s3)) {
                OutlinedTextField(
                    value = name,
                    onValueChange = { name = it },
                    label = { Text(stringResource(R.string.settings_edit_name)) },
                    singleLine = true,
                )
                OutlinedTextField(
                    value = email,
                    onValueChange = { email = it },
                    label = { Text(stringResource(R.string.settings_edit_email)) },
                    singleLine = true,
                )
            }
        },
        confirmButton = {
            TextButton(onClick = { onSave(name, email) }) {
                Text(stringResource(R.string.settings_save), color = ink.accentText)
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) {
                Text(stringResource(R.string.settings_cancel), color = ink.textSecondary)
            }
        },
    )
}

/** A browserless device turns the tap into a no-op instead of a crash. */
private fun openWebPage(context: Context, url: String) {
    try {
        context.startActivity(Intent(Intent.ACTION_VIEW, url.toUri()))
    } catch (_: ActivityNotFoundException) {
        // Nothing can render the page; swallowing beats taking the app down.
    }
}

// MARK-style shared row scaffolding below.

/** Surface + shadow + clip, the grouped-list card idiom of this sheet. */
@Composable
private fun GroupCard(content: @Composable () -> Unit) {
    val ink = InkTheme.colors
    Column(
        Modifier
            .fillMaxWidth()
            .inkShadow(4.dp, InkRadius.lgShape)
            .clip(InkRadius.lgShape)
            .background(ink.bgSurface),
    ) {
        content()
    }
}

@Composable
private fun ToggleRow(
    icon: @Composable () -> Unit,
    title: String,
    subtitle: String,
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
) {
    val ink = InkTheme.colors
    Row(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = InkSpace.s4, vertical = 10.dp),
        horizontalArrangement = Arrangement.spacedBy(InkSpace.s4),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        icon()
        Column(Modifier.weight(1f)) {
            Text(title, style = InkType.ui, color = ink.textDisplay)
            Text(
                subtitle,
                style = InkType.caption,
                color = ink.textTertiary,
                modifier = Modifier.padding(top = 2.dp),
            )
        }
        Switch(
            checked = checked,
            onCheckedChange = onCheckedChange,
            colors = SwitchDefaults.colors(
                checkedTrackColor = ink.accentFill,
                checkedThumbColor = ink.accentInk,
            ),
        )
    }
}

@Composable
private fun DisclosureRow(
    icon: @Composable () -> Unit,
    title: String,
    onClick: () -> Unit,
) {
    val ink = InkTheme.colors
    Row(
        Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick, role = Role.Button)
            .padding(horizontal = InkSpace.s4, vertical = 14.dp),
        horizontalArrangement = Arrangement.spacedBy(InkSpace.s4),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        icon()
        Text(
            title,
            style = InkType.ui,
            color = ink.textDisplay,
            modifier = Modifier.weight(1f),
        )
        Icon(
            Icons.Outlined.ChevronRight,
            null,
            tint = ink.textTertiary,
            modifier = Modifier.size(18.dp),
        )
    }
}

@Composable
private fun Hairline() {
    val ink = InkTheme.colors
    Box(
        Modifier
            .fillMaxWidth()
            .padding(start = InkSpace.s4)
            .height(hairlineThickness())
            .background(ink.borderHairline),
    )
}
