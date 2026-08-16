package app.inkuna.android.ui.stats

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.inkuna.android.R
import app.inkuna.android.ui.components.InkProgressBar
import app.inkuna.android.ui.components.inkShadow
import app.inkuna.android.ui.main.DisplayTitle
import app.inkuna.android.ui.main.EmptyState
import app.inkuna.android.ui.main.ScrollScreen
import app.inkuna.android.ui.main.SectionTitle
import app.inkuna.android.ui.theme.InkRadius
import app.inkuna.android.ui.theme.InkSpace
import app.inkuna.android.ui.theme.InkTheme
import app.inkuna.android.ui.theme.InkType
import java.time.LocalDate
import java.time.format.TextStyle
import java.time.temporal.WeekFields
import java.util.Locale

/** True one-pixel hairline, like iOS's 1/displayScale separators. */
@Composable
fun hairlineThickness(): Dp = with(LocalDensity.current) { (1f / density).dp }

/**
 * The Stats tab: reading facts, the month's reading calendar, and the
 * in-progress list — every number from the core's recorded reading
 * sessions. Until the first answer lands the screen shows its title alone,
 * never zeros pretending to be data.
 */
@Composable
fun StatsScreen(
    innerPadding: PaddingValues,
    model: StatsViewModel = viewModel(),
) {
    val ink = InkTheme.colors
    val state by model.state.collectAsStateWithLifecycle()

    // A reading session may have ended since the numbers were fetched.
    LifecycleResumeEffect(Unit) {
        model.reload()
        onPauseOrDispose {}
    }

    ScrollScreen(innerPadding) {
        DisplayTitle(stringResource(R.string.stats_title))
        val overview = state.overview ?: return@ScrollScreen
        Spacer(Modifier.height(InkSpace.s6))
        // Intrinsic height keeps the three tiles level once a caption wraps
        // — which it does in most translations, and at any raised font scale.
        Row(
            horizontalArrangement = Arrangement.spacedBy(InkSpace.s3),
            modifier = Modifier.height(IntrinsicSize.Min),
        ) {
            FactCard(
                overview.pagesThisWeek.toString(),
                stringResource(R.string.stats_pages_this_week),
                Modifier.weight(1f).fillMaxHeight(),
            )
            FactCard(
                hoursText(overview.minutesThisMonth),
                stringResource(R.string.stats_hours_this_month),
                Modifier.weight(1f).fillMaxHeight(),
            )
            FactCard(
                overview.booksFinishedThisYear.toString(),
                stringResource(R.string.stats_books_this_year),
                Modifier.weight(1f).fillMaxHeight(),
            )
        }
        Spacer(Modifier.height(34.dp))
        // Month and weekday names come from java.time in the reader's locale
        // — spelling them out in English would leave the stats screen
        // untranslated in an otherwise localized app.
        val locale = LocalConfiguration.current.locales[0]
        val today = LocalDate.now()
        val monthName = remember(locale, today.month) {
            today.month.getDisplayName(TextStyle.FULL_STANDALONE, locale)
        }
        SectionTitle(monthName)
        Spacer(Modifier.height(14.dp))
        CalendarCard(monthName, locale, today, overview.readDays)
        Spacer(Modifier.height(34.dp))
        SectionTitle(stringResource(R.string.stats_in_progress))
        Spacer(Modifier.height(6.dp))
        if (state.inProgress.isEmpty()) {
            EmptyState(stringResource(R.string.stats_in_progress_empty))
        }
        state.inProgress.forEach { book ->
            val progress = book.progress ?: 0
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
                InkProgressBar(progress, Modifier.width(92.dp))
                Text(
                    stringResource(R.string.reader_percent, progress),
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

/**
 * Whole and half hours, the design's "6½" voice: 30+ leftover minutes
 * round to the half, never to a decimal.
 */
private fun hoursText(minutes: Int): String {
    val hours = minutes / 60
    val half = minutes % 60 >= 30
    return when {
        hours == 0 && half -> "½"
        hours == 0 -> "0"
        half -> "$hours½"
        else -> "$hours"
    }
}

@Composable
private fun FactCard(value: String, caption: String, modifier: Modifier = Modifier) {
    val ink = InkTheme.colors
    val factLabel = stringResource(R.string.a11y_fact, value, caption)
    Column(
        modifier = modifier
            .inkShadow(2.dp, InkRadius.mdShape)
            .clip(InkRadius.mdShape)
            .background(ink.bgSurface)
            // One fact, one stop: "214" and "pages this week" are meaningless
            // as separate announcements.
            .semantics(mergeDescendants = true) { contentDescription = factLabel }
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
private fun CalendarCard(
    monthName: String,
    locale: Locale,
    today: LocalDate,
    readDays: Set<Int>,
) {
    val ink = InkTheme.colors
    // The grid starts on the locale's first day of the week, with that
    // locale's own narrow weekday initials; the leading blanks put the 1st
    // in its true column.
    val firstDay = remember(locale) { WeekFields.of(locale).firstDayOfWeek }
    val leadingBlanks =
        (today.withDayOfMonth(1).dayOfWeek.value - firstDay.value + 7) % 7
    val cells = buildList {
        repeat(leadingBlanks) { add(null) }
        (1..today.lengthOfMonth()).forEach { add(it) }
        while (size % 7 != 0) add(null)
    }
    val weekdays = remember(locale) {
        (0L..6L).map { offset ->
            firstDay.plus(offset).getDisplayName(TextStyle.NARROW_STANDALONE, locale)
        }
    }
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .inkShadow(2.dp, InkRadius.lgShape)
            .clip(InkRadius.lgShape)
            .background(ink.bgSurface)
            .padding(horizontal = 14.dp, vertical = InkSpace.s4),
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(InkSpace.s1)) {
            weekdays.forEach { day ->
                Text(
                    day,
                    style = InkType.caption,
                    color = ink.textTertiary,
                    textAlign = TextAlign.Center,
                    modifier = Modifier
                        .weight(1f)
                        .padding(bottom = InkSpace.s1)
                        // Column headers add nothing once each cell says its
                        // own date.
                        .clearAndSetSemantics {},
                )
            }
        }
        cells.chunked(7).forEach { week ->
            Row(
                horizontalArrangement = Arrangement.spacedBy(InkSpace.s1),
                modifier = Modifier.padding(top = InkSpace.s1),
            ) {
                week.forEach { day ->
                    DayCell(
                        day = day,
                        monthName = monthName,
                        today = today.dayOfMonth,
                        didRead = day != null && day in readDays,
                        modifier = Modifier.weight(1f),
                    )
                }
            }
        }
        Text(
            pluralStringResource(R.plurals.stats_evenings, readDays.size, readDays.size),
            style = InkType.caption,
            color = ink.textTertiary,
            modifier = Modifier.padding(top = 10.dp),
        )
    }
}

@Composable
private fun DayCell(
    day: Int?,
    monthName: String,
    today: Int,
    didRead: Boolean,
    modifier: Modifier = Modifier,
) {
    val ink = InkTheme.colors
    val isToday = day == today
    // "Read that evening" is carried only by a 4dp accent dot; without a text
    // alternative the whole calendar is a column of bare numerals.
    val cellLabel = when {
        day == null -> ""
        didRead -> stringResource(R.string.a11y_calendar_day_read, monthName, day)
        else -> stringResource(R.string.a11y_calendar_day, monthName, day)
    }
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
        modifier = modifier
            .defaultMinSize(minHeight = 36.dp)
            .clip(InkRadius.smShape)
            .background(if (isToday) ink.accentSoft else Color.Transparent)
            .clearAndSetSemantics { if (day != null) contentDescription = cellLabel },
    ) {
        Text(
            day?.toString() ?: "",
            style = InkType.label.copy(fontWeight = androidx.compose.ui.text.font.FontWeight.Normal),
            color = when {
                day == null -> Color.Transparent
                day > today -> ink.textTertiary
                else -> ink.textDisplay
            },
        )
        Box(
            Modifier
                .padding(top = 3.dp)
                .size(4.dp)
                .clip(CircleShape)
                .background(if (didRead) ink.accent else Color.Transparent)
        )
    }
}
