package app.inkuna.android

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextAlign
import app.inkuna.core.Bookshelf
import app.inkuna.core.Publication
import app.inkuna.core.coreVersion
import java.io.File

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val library = Bookshelf.open(File(filesDir, "inkuna.db").absolutePath)
        setContent {
            MaterialTheme {
                LibraryScreen(library)
            }
        }
    }
}

/**
 * The shelf. Currently a plain list fed by the Rust core; the crafted
 * presentation replaces this once Readium lands.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun LibraryScreen(library: Bookshelf) {
    val publications: List<Publication> = remember { runCatching { library.list() }.getOrDefault(emptyList()) }
    Scaffold(
        topBar = { TopAppBar(title = { Text("Inkuna") }) }
    ) { padding ->
        if (publications.isEmpty()) {
            Box(
                modifier = Modifier.fillMaxSize().padding(padding),
                contentAlignment = Alignment.Center
            ) {
                Text(
                    text = "Where ink meets moonlight.\ncore ${coreVersion()}",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    textAlign = TextAlign.Center
                )
            }
        } else {
            LazyColumn(modifier = Modifier.fillMaxSize().padding(padding)) {
                items(publications, key = { it.id }) { publication ->
                    ListItem(
                        headlineContent = { Text(publication.title) },
                        supportingContent = { Text(publication.authors.joinToString(", ")) }
                    )
                }
            }
        }
    }
}
