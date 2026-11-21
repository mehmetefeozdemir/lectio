package org.lectio.android

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import org.lectio.android.ui.LectioRoot
import org.lectio.android.ui.theme.LectioTheme

/** The one activity: Compose draws everything, edge to edge. */
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        val app = application as LectioApplication
        setContent {
            LectioTheme {
                Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
                    val graph = app.graph
                    if (graph != null) {
                        LectioRoot(graph)
                    } else {
                        Column(
                            Modifier.fillMaxSize().padding(24.dp),
                            verticalArrangement = Arrangement.Center,
                            horizontalAlignment = Alignment.CenterHorizontally,
                        ) {
                            Text("Lectio can't open your notes", style = MaterialTheme.typography.titleMedium)
                            Text(app.startupProblem.orEmpty(), textAlign = TextAlign.Center)
                        }
                    }
                }
            }
        }
    }
}
