package app.inkuna.android.ui.onboarding

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.inkuna.android.R
import app.inkuna.android.ui.components.InkButton
import app.inkuna.android.ui.components.InkButtonSize
import app.inkuna.android.ui.theme.InkTheme
import app.inkuna.android.ui.theme.InkType

@Composable
fun WelcomeScreen(onBegin: () -> Unit) {
    val ink = InkTheme.colors
    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(ink.bgApp)
            .padding(horizontal = 36.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = androidx.compose.foundation.layout.Arrangement.Center,
    ) {
        BrandMark()
        Spacer(Modifier.height(26.dp))
        Text(
            stringResource(R.string.app_name),
            style = InkType.display.copy(fontSize = 54.sp, lineHeight = 58.sp),
            color = ink.textDisplay,
        )
        Text(
            stringResource(R.string.welcome_tagline),
            style = InkType.reading,
            color = ink.textSecondary,
            textAlign = TextAlign.Center,
            modifier = Modifier
                .padding(top = 14.dp)
                .widthIn(max = 384.dp),
        )
        Spacer(Modifier.height(44.dp))
        InkButton(
            text = stringResource(R.string.welcome_begin),
            onClick = onBegin,
            size = InkButtonSize.Large,
        )
    }
}

/**
 * The canonical Inkuna mark (assets/brand/inkuna-mark.svg): an ink disc
 * with a paper moon floating inside, tangent to the rim at the upper
 * right — scaled from the 1024 canvas to 44dp and drawn in theme ink so
 * it reads in both day and night.
 */
@Composable
private fun BrandMark() {
    val ink = InkTheme.colors
    Box(Modifier.size(44.dp)) {
        Box(
            Modifier
                .size(44.dp)
                .clip(CircleShape)
                .background(ink.textDisplay.copy(alpha = 0.9f))
        )
        Box(
            Modifier
                .offset(x = 16.8.dp, y = 2.1.dp)
                .size(27.2.dp)
                .clip(CircleShape)
                .background(ink.bgApp)
        )
    }
}
