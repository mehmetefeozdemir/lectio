package org.lectio.android.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.selection.selectable
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import org.lectio.android.models.SessionViewModel
import org.lectio.android.models.TranscriptionViewModel
import org.lectio.android.models.TranscriptionViewModel.Phase
import org.lectio.android.support.TimeText

/** The transcript, made on this device; each line plays from its moment. */
@Composable
fun TranscriptTab(session: SessionViewModel, job: TranscriptionViewModel, playFrom: (ULong) -> Unit) {
    val state by session.state.collectAsStateWithLifecycle()
    val jobState by job.state.collectAsStateWithLifecycle()
    val segments = state.detail?.transcript.orEmpty()

    LazyColumn(Modifier.fillMaxSize()) {
        item {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("Model", style = MaterialTheme.typography.labelLarge)
                job.available.forEach { model ->
                    Row(
                        Modifier.fillMaxWidth()
                            .selectable(selected = jobState.modelName == model.name, enabled = !jobState.isRunning, role = Role.RadioButton) { job.chooseModel(model.name) }
                            .padding(vertical = 2.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        RadioButton(selected = jobState.modelName == model.name, onClick = null, enabled = !jobState.isRunning)
                        Text(model.displayName, modifier = Modifier.padding(start = 8.dp))
                    }
                }
                PhaseLine(jobState.phase)
                if (jobState.isRunning) {
                    OutlinedButton(onClick = job::cancel) { Text("Stop") }
                } else {
                    Button(onClick = job::transcribe, enabled = state.recordingPath != null, modifier = Modifier.testTag("transcript.start")) { Text("Transcribe") }
                }
                Text(
                    "Transcription runs on this device; nothing is uploaded. The first time, the model is downloaded once.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            HorizontalDivider()
            if (segments.isEmpty()) {
                Text(
                    if (state.recordingPath == null) "Record the lecture first." else "No transcript yet.",
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(16.dp),
                )
            }
        }
        itemsIndexed(segments) { _, segment ->
            Row(
                Modifier.fillMaxWidth()
                    .clickable(enabled = state.recordingPath != null) { playFrom(segment.startMs) }
                    .padding(horizontal = 16.dp, vertical = 8.dp)
                    .semantics(mergeDescendants = true) {
                        contentDescription = segment.text
                        onClick(label = "Play from ${TimeText.spoken(segment.startMs)}") { playFrom(segment.startMs); true }
                    },
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Text(TimeText.clock(segment.startMs), fontFamily = FontFamily.Monospace, color = MaterialTheme.colorScheme.onSurfaceVariant)
                Text(segment.text, modifier = Modifier.weight(1f))
            }
        }
    }
}

@Composable
private fun PhaseLine(phase: Phase) {
    val live = Modifier.semantics { liveRegion = LiveRegionMode.Polite }
    when (phase) {
        Phase.Idle -> Unit
        Phase.Loading -> Row(live, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CircularProgressIndicator(Modifier.padding(2.dp))
            Text("Loading the model")
        }
        is Phase.Downloading -> Progress("Downloading the model", phase.fraction)
        is Phase.Transcribing -> Progress("Transcribing", phase.fraction)
        is Phase.Finished -> Text(phase.text, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = live.testTag("transcript.done"))
        is Phase.Failed -> Text(phase.message, color = MaterialTheme.colorScheme.error, modifier = live)
    }
}

@Composable
private fun Progress(title: String, fraction: Double) {
    Column(Modifier.semantics(mergeDescendants = true) { contentDescription = "$title, ${(fraction * 100).toInt()} percent" }) {
        Text(title)
        LinearProgressIndicator(progress = { fraction.toFloat() }, modifier = Modifier.fillMaxWidth())
    }
}
