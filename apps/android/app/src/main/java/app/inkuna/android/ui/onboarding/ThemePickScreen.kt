package app.inkuna.android.ui.onboarding

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.inkuna.android.R
import app.inkuna.android.ui.components.InkButton
import app.inkuna.android.ui.components.InkButtonSize
import app.inkuna.android.ui.components.inkShadow
import app.inkuna.android.ui.main.DisplayTitle
import app.inkuna.android.ui.main.EyebrowText
import app.inkuna.android.ui.theme.InkRadius
import app.inkuna.android.ui.theme.InkTheme
import app.inkuna.android.ui.theme.InkType
import app.inkuna.android.ui.theme.ReadingTheme

@Composable
fun ThemePickScreen(
    selectedTheme: ReadingTheme,
    onPick: (ReadingTheme) -> Unit,
    onContinue: () -> Unit,
) {
    val ink = InkTheme.colors
    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(ink.bgApp)
            .safeDrawingPadding()
            .padding(horizontal = 24.dp)
            .padding(top = 40.dp, bottom = 16.dp),
    ) {
        // The ritual must stay completable: at large font scales the title
        // and two preview cards outgrow the viewport, so they scroll and
        // Continue stays pinned outside the scrolling region.
        Column(
            modifier = Modifier
                .weight(1f)
                .verticalScroll(rememberScrollState()),
        ) {
            EyebrowText(stringResource(R.string.themepick_eyebrow))
            Spacer(Modifier.height(6.dp))
            DisplayTitle(stringResource(R.string.themepick_title))
            Spacer(Modifier.height(28.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                // Paper and Moon are the canonical pair; selection matches by
                // night parity so a returning Calm/Quiet reader sees the right
                // card ringed.
                ThemePreviewCard(
                    theme = ReadingTheme.Paper,
                    chosen = !selectedTheme.isNight,
                    onPick = onPick,
                    modifier = Modifier.weight(1f),
                )
                ThemePreviewCard(
                    theme = ReadingTheme.Moon,
                    chosen = selectedTheme.isNight,
                    onPick = onPick,
                    modifier = Modifier.weight(1f),
                )
            }
            Spacer(Modifier.height(16.dp))
            Text(
                stringResource(R.string.themepick_hint),
                style = InkType.caption,
                color = ink.textTertiary,
            )
        }
        Spacer(Modifier.height(16.dp))
        Box(modifier = Modifier.align(Alignment.CenterHorizontally)) {
            InkButton(
                text = stringResource(R.string.themepick_continue),
                onClick = onContinue,
                size = InkButtonSize.Large,
            )
        }
    }
}

@Composable
private fun ThemePreviewCard(
    theme: ReadingTheme,
    chosen: Boolean,
    onPick: (ReadingTheme) -> Unit,
    modifier: Modifier = Modifier,
) {
    val ink = InkTheme.colors
    val haptics = LocalHapticFeedback.current
    val name = stringResource(theme.nameRes)
    val subtitle = stringResource(theme.subtitleRes)
    val tileLabel = stringResource(R.string.a11y_theme_tile, name, subtitle)
    Column(
        modifier = modifier
            .inkShadow(if (chosen) 6.dp else 2.dp, InkRadius.lgShape)
            .clip(InkRadius.lgShape)
            .background(theme.background)
            .border(
                3.dp,
                if (chosen) ink.accent.copy(alpha = 0.35f) else Color.Transparent,
                InkRadius.lgShape,
            )
            .clickable(role = Role.Button) {
                haptics.performHapticFeedback(HapticFeedbackType.SegmentTick)
                onPick(theme)
            }
            // Without this the app's first interactive screen reads the whole
            // sample paragraph aloud before naming the theme.
            .clearAndSetSemantics {
                contentDescription = tileLabel
                role = Role.Button
                selected = chosen
            }
            .padding(start = 16.dp, end = 16.dp, top = 18.dp, bottom = 16.dp),
    ) {
        Text(
            stringResource(R.string.themepick_sample),
            style = InkType.reading.copy(fontSize = 13.sp, lineHeight = 21.sp),
            color = theme.foreground,
            maxLines = 5,
            overflow = TextOverflow.Ellipsis,
        )
        Spacer(Modifier.height(14.dp))
        Text(name, style = InkType.ui, color = theme.foreground)
        Text(
            subtitle,
            style = InkType.caption,
            color = theme.foreground.copy(alpha = 0.62f),
            modifier = Modifier.padding(top = 2.dp),
        )
    }
}
