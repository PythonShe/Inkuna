package app.inkuna.android.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Search
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import app.inkuna.android.ui.theme.InkRadius
import app.inkuna.android.ui.theme.InkTheme
import app.inkuna.android.ui.theme.InkType

/** Recessed pill search field. */
@Composable
fun InkSearchField(
    value: String,
    onValueChange: (String) -> Unit,
    placeholder: String,
    modifier: Modifier = Modifier,
) {
    val ink = InkTheme.colors
    val textStyle = InkType.ui.copy(fontWeight = FontWeight.Normal, color = ink.textBody)
    Row(
        modifier = modifier
            .fillMaxWidth()
            .clip(InkRadius.pillShape)
            .background(ink.bgRecessed)
            .padding(horizontal = 16.dp, vertical = 12.dp),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            Icons.Outlined.Search,
            contentDescription = null,
            tint = ink.textTertiary,
            modifier = Modifier.size(19.dp),
        )
        // Filtering is live, so the Search key has nothing to submit — but it
        // must still put the keyboard away rather than sit inert.
        val focusManager = LocalFocusManager.current
        BasicTextField(
            value = value,
            onValueChange = onValueChange,
            textStyle = textStyle,
            singleLine = true,
            cursorBrush = SolidColor(ink.accentText),
            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
            keyboardActions = KeyboardActions(onSearch = { focusManager.clearFocus() }),
            modifier = Modifier
                .weight(1f)
                // The placeholder stops merging in once text is typed, so the
                // field would otherwise announce only its own contents.
                .semantics { contentDescription = placeholder },
            decorationBox = { inner ->
                Box {
                    if (value.isEmpty()) {
                        Text(placeholder, style = textStyle, color = ink.textTertiary)
                    }
                    inner()
                }
            },
        )
    }
}
