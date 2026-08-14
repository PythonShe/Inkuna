package app.inkuna.android.ui.stats

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.inkuna.android.R
import app.inkuna.android.model.PlaceholderLibrary
import app.inkuna.android.ui.components.InkProgressBar
import app.inkuna.android.ui.main.DisplayTitle
import app.inkuna.android.ui.main.ScrollScreen
import app.inkuna.android.ui.main.SectionTitle
import app.inkuna.android.ui.theme.InkRadius
import app.inkuna.android.ui.theme.InkSpace
import app.inkuna.android.ui.theme.InkTheme
import app.inkuna.android.ui.theme.InkType

/** True one-pixel hairline, like iOS's 1/displayScale separators. */
@Composable
fun hairlineThickness(): Dp = with(LocalDensity.current) { (1f / density).dp }

@Composable
fun StatsScreen(innerPadding: PaddingValues) {
    val ink = InkTheme.colors
    // TODO(core): all stats derive from recorded reading sessions; the
    // calendar is prototype data, not the real date.
    ScrollScreen(innerPadding) {
        DisplayTitle(stringResource(R.string.stats_title))
        Spacer(Modifier.height(InkSpace.s6))
        Row(horizontalArrangement = Arrangement.spacedBy(InkSpace.s3)) {
            PlaceholderLibrary.facts.forEach { (value, caption) ->
                FactCard(value, caption, Modifier.weight(1f))
            }
        }
        Spacer(Modifier.height(34.dp))
        SectionTitle(PlaceholderLibrary.calendarMonthTitle)
        Spacer(Modifier.height(14.dp))
        CalendarCard()
        Spacer(Modifier.height(34.dp))
        SectionTitle(stringResource(R.string.stats_in_progress))
        Spacer(Modifier.height(6.dp))
        PlaceholderLibrary.books.forEach { book ->
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(14.dp),
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(vertical = 13.dp),
            ) {
                Text(
                    book.title,
                    style = InkType.heading.copy(fontSize = 15.sp, lineHeight = 20.sp),
                    color = ink.textDisplay,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                InkProgressBar(book.progress, Modifier.width(92.dp))
                Text(
                    book.percentText,
                    style = InkType.caption,
                    color = ink.textTertiary,
                    textAlign = TextAlign.End,
                    maxLines = 1,
                    modifier = Modifier.widthIn(min = 34.dp),
                )
            }
            Box(
                Modifier
                    .fillMaxWidth()
                    .height(hairlineThickness())
                    .background(ink.borderHairline)
            )
        }
    }
}

@Composable
private fun FactCard(value: String, caption: String, modifier: Modifier = Modifier) {
    val ink = InkTheme.colors
    Column(
        modifier = modifier
            .shadow(2.dp, InkRadius.mdShape)
            .clip(InkRadius.mdShape)
            .background(ink.bgSurface)
            .padding(horizontal = 14.dp, vertical = InkSpace.s4),
    ) {
        Text(value, style = InkType.displaySmall, color = ink.textDisplay)
        Text(
            caption,
            style = InkType.caption,
            color = ink.textSecondary,
            maxLines = 2,
            modifier = Modifier.padding(top = 6.dp),
        )
    }
}

@Composable
private fun CalendarCard() {
    val ink = InkTheme.colors
    val cells = buildList {
        repeat(PlaceholderLibrary.calendarLeadingBlanks) { add(null) }
        (1..PlaceholderLibrary.calendarDayCount).forEach { add(it) }
        while (size % 7 != 0) add(null)
    }
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .shadow(2.dp, InkRadius.lgShape)
            .clip(InkRadius.lgShape)
            .background(ink.bgSurface)
            .padding(horizontal = 14.dp, vertical = InkSpace.s4),
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(InkSpace.s1)) {
            PlaceholderLibrary.calendarWeekdays.forEach { day ->
                Text(
                    day,
                    style = InkType.caption,
                    color = ink.textTertiary,
                    textAlign = TextAlign.Center,
                    modifier = Modifier
                        .weight(1f)
                        .padding(bottom = InkSpace.s1),
                )
            }
        }
        cells.chunked(7).forEach { week ->
            Row(
                horizontalArrangement = Arrangement.spacedBy(InkSpace.s1),
                modifier = Modifier.padding(top = InkSpace.s1),
            ) {
                week.forEach { day -> DayCell(day, Modifier.weight(1f)) }
            }
        }
        Text(
            PlaceholderLibrary.calendarCaption,
            style = InkType.caption,
            color = ink.textTertiary,
            modifier = Modifier.padding(top = 10.dp),
        )
    }
}

@Composable
private fun DayCell(day: Int?, modifier: Modifier = Modifier) {
    val ink = InkTheme.colors
    val isToday = day == PlaceholderLibrary.calendarToday
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
        modifier = modifier
            .defaultMinSize(minHeight = 36.dp)
            .clip(InkRadius.smShape)
            .background(if (isToday) ink.accentSoft else Color.Transparent),
    ) {
        Text(
            day?.toString() ?: "",
            style = InkType.label.copy(fontWeight = androidx.compose.ui.text.font.FontWeight.Normal),
            color = when {
                day == null -> Color.Transparent
                day > PlaceholderLibrary.calendarToday -> ink.textTertiary
                else -> ink.textDisplay
            },
        )
        Box(
            Modifier
                .padding(top = 3.dp)
                .size(4.dp)
                .clip(CircleShape)
                .background(
                    if (day != null && day in PlaceholderLibrary.calendarReadDays) ink.accent
                    else Color.Transparent
                )
        )
    }
}
