package org.lectio.android.ui

import androidx.activity.compose.PredictiveBackHandler
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.width
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.VerticalDivider
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.CancellationException
import org.lectio.android.AppGraph
import org.lectio.android.models.HomeViewModel

/** At this width and wider (tablets, unfolded phones, landscape), list and lecture sit side by side. */
private val TWO_PANE_MIN_WIDTH = 600.dp
private val LIST_PANE_WIDTH = 360.dp

/**
 * Lectures and the open lecture: side by side on wide screens, one at a time
 * on phones. On a phone, the back gesture slides the lecture away as the
 * finger moves (predictive back) and returns to the list.
 */
@Composable
fun LectioRoot(graph: AppGraph) {
    val home: HomeViewModel = viewModel { HomeViewModel(graph.library, graph::changed) }
    val revision by graph.revision.collectAsStateWithLifecycle()
    LaunchedEffect(revision) { home.reload() }
    var selected by rememberSaveable { mutableStateOf<String?>(null) }

    BoxWithConstraints(Modifier.fillMaxSize()) {
        if (maxWidth >= TWO_PANE_MIN_WIDTH) {
            Row(Modifier.fillMaxSize()) {
                Box(Modifier.width(LIST_PANE_WIDTH).fillMaxHeight()) {
                    HomeScreen(home, selected) { selected = it }
                }
                VerticalDivider()
                Box(Modifier.weight(1f).fillMaxHeight()) {
                    val open = selected
                    if (open != null) {
                        SessionScreen(graph, open, onBack = null)
                    } else {
                        Text(
                            "Choose a lecture, or start a new one.",
                            style = MaterialTheme.typography.bodyLarge,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            modifier = Modifier.align(Alignment.Center),
                        )
                    }
                }
            }
        } else {
            var backProgress by remember { mutableFloatStateOf(0f) }
            PredictiveBackHandler(enabled = selected != null) { events ->
                try {
                    events.collect { backProgress = it.progress }
                    selected = null
                } catch (e: CancellationException) {
                    throw e
                } finally {
                    backProgress = 0f
                }
            }
            val open = selected
            if (open == null) {
                HomeScreen(home, null) { selected = it }
            } else {
                SessionScreen(
                    graph,
                    open,
                    onBack = { selected = null },
                    modifier = Modifier.graphicsLayer {
                        translationX = backProgress * size.width * BACK_SLIDE
                        alpha = 1f - backProgress * BACK_FADE
                    },
                )
            }
        }
    }
}

private const val BACK_SLIDE = 0.15f
private const val BACK_FADE = 0.3f
