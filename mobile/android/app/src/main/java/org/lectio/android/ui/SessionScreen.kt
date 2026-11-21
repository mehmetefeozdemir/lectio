package org.lectio.android.ui

import android.Manifest
import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.FiberManualRecord
import androidx.compose.material.icons.filled.Pause
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Share
import androidx.compose.material.icons.filled.Stop
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.PrimaryScrollableTabRow
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderState
import androidx.compose.material3.Tab
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import androidx.core.content.FileProvider
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import java.io.File
import kotlinx.coroutines.launch
import org.lectio.android.AppGraph
import org.lectio.android.audio.PlayerController
import org.lectio.android.audio.RecorderController
import org.lectio.android.models.SessionViewModel
import org.lectio.android.models.StudyViewModel
import org.lectio.android.models.TranscriptionViewModel
import org.lectio.android.support.TimeText
import uniffi.lectio_ffi.ExportFormat

private enum class Tab(val title: String) { NOTES("Notes"), SLIDES("Slides"), TRANSCRIPT("Transcript"), STUDY("Study") }

/**
 * One lecture: the audio bar on top, then Notes, Slides, Transcript and
 * Study. New notes are stamped with the recording's current moment (while
 * recording) or playback's (while reviewing).
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SessionScreen(graph: AppGraph, sessionId: String, onBack: (() -> Unit)?, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val changed: () -> Unit = graph::changed
    val session: SessionViewModel = viewModel(key = "session-$sessionId") {
        SessionViewModel(sessionId, graph.library, changed, graph.exportDirectory)
    }
    val transcription: TranscriptionViewModel = viewModel(key = "transcription-$sessionId") {
        TranscriptionViewModel(sessionId, graph.library, graph.models, stored = { session.reload() })
    }
    val study: StudyViewModel = viewModel(key = "study-$sessionId") {
        StudyViewModel(sessionId, graph.library, graph.models, changed)
    }
    val state by session.state.collectAsStateWithLifecycle()
    val recorder by graph.recorder.state.collectAsStateWithLifecycle()
    val player by graph.player.state.collectAsStateWithLifecycle()
    val revision by graph.revision.collectAsStateWithLifecycle()
    var tab by rememberSaveable { mutableStateOf(Tab.NOTES) }

    LaunchedEffect(revision) {
        session.reload()
        session.state.value.recordingPath?.let { graph.player.prepare(sessionId, it) }
    }

    val offsetNow: () -> ULong = {
        when {
            graph.recorder.isRecording(sessionId) -> graph.recorder.state.value.elapsedMs
            graph.player.isLoaded(sessionId) -> graph.player.state.value.positionMs
            else -> 0uL
        }
    }
    val playFrom: (ULong) -> Unit = { ms -> state.recordingPath?.let { graph.player.play(sessionId, it, ms) } }

    Scaffold(
        modifier = modifier,
        topBar = {
            TopAppBar(
                title = { Text(state.title, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                navigationIcon = {
                    if (onBack != null) {
                        IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back to lectures") }
                    }
                },
                actions = { ExportMenu(session, state.isExporting) { file, format -> share(context, file, format) } },
            )
        },
    ) { padding ->
        Column(Modifier.padding(padding).fillMaxSize()) {
            AudioBar(graph, sessionId, state.recordingPath, state.detail?.info?.durationMs, recorder, player)
            state.status?.let {
                Text(
                    it,
                    style = MaterialTheme.typography.bodySmall,
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp).semantics { liveRegion = LiveRegionMode.Polite }.testTag("session.status"),
                )
            }
            // Scrollable, so the labels keep their full text at large font sizes.
            PrimaryScrollableTabRow(selectedTabIndex = tab.ordinal, edgePadding = 0.dp) {
                Tab.entries.forEach { t ->
                    Tab(selected = tab == t, onClick = { tab = t }, text = { Text(t.title, maxLines = 1, softWrap = false) }, modifier = Modifier.testTag("tab.${t.name.lowercase()}"))
                }
            }
            when (tab) {
                Tab.NOTES -> NotesTab(session, graph, offsetNow, playFrom)
                Tab.SLIDES -> SlidesTab(session, offsetNow)
                Tab.TRANSCRIPT -> TranscriptTab(session, transcription, playFrom)
                Tab.STUDY -> StudyTab(session, study, playFrom)
            }
        }
    }

    ProblemDialog(state.problem ?: recorder.problem ?: player.problem) {
        session.clearProblem()
        graph.recorder.clearProblem()
        graph.player.clearProblem()
    }
}

/** Record; or, once recorded, play with a scrubber and speed. */
@Composable
private fun AudioBar(
    graph: AppGraph,
    sessionId: String,
    recordingPath: String?,
    savedDurationMs: ULong?,
    recorder: RecorderController.State,
    player: PlayerController.State,
) {
    Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp)) {
        when {
            recorder.sessionId == sessionId && recorder.phase != RecorderController.Phase.IDLE -> Recording(graph, recorder)
            recordingPath != null -> Playback(graph, sessionId, recordingPath, savedDurationMs, player)
            else -> RecordButton(graph, sessionId, enabled = recorder.phase == RecorderController.Phase.IDLE)
        }
    }
}

@Composable
private fun Recording(graph: AppGraph, recorder: RecorderController.State) {
    val interrupted = recorder.phase == RecorderController.Phase.INTERRUPTED
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Icon(Icons.Filled.FiberManualRecord, contentDescription = null, tint = MaterialTheme.colorScheme.error)
        Column(Modifier.weight(1f).semantics(mergeDescendants = true) {}) {
            Text(if (interrupted) "Paused by a call" else "Recording", style = MaterialTheme.typography.titleSmall)
            val input = if (recorder.inputName.isEmpty()) "" else " · ${recorder.inputName}"
            Text(TimeText.clock(recorder.elapsedMs) + input, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.testTag("audio.elapsed"))
        }
        if (interrupted) OutlinedButton(onClick = graph.recorder::resume) { Text("Resume") }
        Button(onClick = { graph.recorder.stop() }, modifier = Modifier.testTag("audio.stop")) {
            Icon(Icons.Filled.Stop, contentDescription = null)
            Text("Stop", modifier = Modifier.padding(start = 4.dp))
        }
    }
}

@Composable
private fun Playback(graph: AppGraph, sessionId: String, path: String, savedDurationMs: ULong?, player: PlayerController.State) {
    val loaded = player.sessionId == sessionId
    val playing = loaded && player.isPlaying
    val position = if (loaded) player.positionMs else 0uL
    val duration = if (loaded) player.durationMs else savedDurationMs ?: 0uL
    Row(verticalAlignment = Alignment.CenterVertically) {
        IconButton(
            onClick = { if (playing) graph.player.pause() else graph.player.play(sessionId, path) },
            modifier = Modifier.testTag("audio.play").semantics { stateDescription = if (playing) "Playing" else "Paused" },
        ) {
            Icon(if (playing) Icons.Filled.Pause else Icons.Filled.PlayArrow, contentDescription = if (playing) "Pause" else "Play")
        }
        PositionSlider(position, duration, Modifier.weight(1f)) { ms ->
            if (!loaded) graph.player.prepare(sessionId, path)
            graph.player.seek(ms)
        }
        SpeedMenu(graph, player.speedPercent)
    }
    Row(Modifier.fillMaxWidth()) {
        Text(TimeText.clock(position), style = MaterialTheme.typography.labelSmall, modifier = Modifier.weight(1f))
        Text(TimeText.clock(duration), style = MaterialTheme.typography.labelSmall)
    }
}

/**
 * The scrubber: follows playback, and seeks once when a drag ends (not on
 * every movement, which would restart the audio each time).
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun PositionSlider(position: ULong, duration: ULong, modifier: Modifier, seek: (ULong) -> Unit) {
    val slider = remember(duration) { SliderState(0f, 0, 0f..maxOf(duration.toFloat(), 1f)) }
    var dragging by remember { mutableStateOf(false) }
    LaunchedEffect(position, dragging) { if (!dragging) slider.value = position.toFloat() }
    Slider(
        state = slider,
        onValueChange = { value ->
            dragging = true
            slider.value = value
        },
        onValueChangeFinished = {
            dragging = false
            seek(slider.value.toLong().coerceAtLeast(0).toULong())
        },
        modifier = modifier.semantics { contentDescription = "Position"; stateDescription = TimeText.spoken(position) },
    )
}

@Composable
private fun SpeedMenu(graph: AppGraph, percent: Int) {
    var open by remember { mutableStateOf(false) }
    val text = speedText(percent)
    TextButton(onClick = { open = true }, modifier = Modifier.semantics { contentDescription = "Speed, $text" }) { Text(text) }
    DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
        PlayerController.SPEEDS.forEach { speed ->
            DropdownMenuItem(text = { Text(speedText(speed)) }, onClick = { graph.player.setSpeed(speed); open = false })
        }
    }
}

private fun speedText(percent: Int): String = (percent / 100.0).toString().removeSuffix(".0") + "×"

/**
 * The record button. The first time, Lectio explains why it needs the
 * microphone (and notifications, for the "recording" notification) before
 * Android asks; if the answer was no, it says where to change it.
 */
@Composable
private fun RecordButton(graph: AppGraph, sessionId: String, enabled: Boolean) {
    val context = LocalContext.current
    var explaining by remember { mutableStateOf(false) }
    var refused by remember { mutableStateOf(false) }
    val ask = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { answers ->
        if (answers[Manifest.permission.RECORD_AUDIO] == true) graph.recorder.start(sessionId) else refused = true
    }
    Button(
        onClick = {
            when {
                !graph.needsMicrophonePermission || hasMicrophone(context) -> graph.recorder.start(sessionId)
                else -> explaining = true
            }
        },
        enabled = enabled,
        colors = ButtonDefaults.buttonColors(containerColor = MaterialTheme.colorScheme.error, contentColor = MaterialTheme.colorScheme.onError),
        modifier = Modifier.fillMaxWidth().testTag("audio.record").semantics {
            contentDescription = "Record. Notes you add are linked to this moment."
        },
    ) {
        Icon(Icons.Filled.FiberManualRecord, contentDescription = null, modifier = Modifier.size(18.dp))
        Text("Record", modifier = Modifier.padding(start = 6.dp))
    }
    if (explaining) {
        AlertDialog(
            onDismissRequest = { explaining = false },
            title = { Text("Record the lecture") },
            text = {
                Text(
                    "Lectio records the lecture so each note can play back the moment it was written. " +
                        "Android will ask for the microphone next, and for notifications, so a notification can show while it records with the screen off.",
                )
            },
            confirmButton = {
                TextButton(onClick = {
                    explaining = false
                    ask.launch(recordingPermissions())
                }) { Text("Continue") }
            },
            dismissButton = { TextButton(onClick = { explaining = false }) { Text("Not now") } },
        )
    }
    if (refused) {
        AlertDialog(
            onDismissRequest = { refused = false },
            title = { Text("The microphone is off for Lectio") },
            text = { Text("To record, allow the microphone in Settings › Apps › Lectio › Permissions.") },
            confirmButton = {
                TextButton(onClick = {
                    refused = false
                    context.startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.fromParts("package", context.packageName, null)))
                }) { Text("Open Settings") }
            },
            dismissButton = { TextButton(onClick = { refused = false }) { Text("Close") } },
        )
    }
}

private fun hasMicrophone(context: Context) =
    ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED

private fun recordingPermissions(): Array<String> =
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
        arrayOf(Manifest.permission.RECORD_AUDIO, Manifest.permission.POST_NOTIFICATIONS)
    } else {
        arrayOf(Manifest.permission.RECORD_AUDIO)
    }

/** PDF, Word or Markdown, then the Android share sheet. */
@Composable
private fun ExportMenu(session: SessionViewModel, exporting: Boolean, share: (File, ExportFormat) -> Unit) {
    var open by remember { mutableStateOf(false) }
    var includeTranscript by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    if (exporting) {
        CircularProgressIndicator(Modifier.size(24.dp).semantics { contentDescription = "Exporting" })
        return
    }
    IconButton(onClick = { open = true }, modifier = Modifier.testTag("session.export")) {
        Icon(Icons.Filled.Share, contentDescription = "Export")
    }
    DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
        DropdownMenuItem(
            text = { Text("Include the transcript") },
            leadingIcon = { Checkbox(checked = includeTranscript, onCheckedChange = null) },
            onClick = { includeTranscript = !includeTranscript },
        )
        listOf(ExportFormat.PDF to "PDF", ExportFormat.DOCX to "Word (.docx)", ExportFormat.MARKDOWN to "Markdown").forEach { (format, name) ->
            DropdownMenuItem(
                text = { Text(name) },
                onClick = {
                    open = false
                    scope.launch { session.export(format, includeTranscript)?.let { share(it, format) } }
                },
                modifier = Modifier.testTag("export.${format.name.lowercase()}"),
            )
        }
    }
}

private fun share(context: Context, file: File, format: ExportFormat) {
    val mime = when (format) {
        ExportFormat.PDF -> "application/pdf"
        ExportFormat.DOCX -> "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        ExportFormat.MARKDOWN -> "text/markdown"
    }
    val uri = FileProvider.getUriForFile(context, "${context.packageName}.files", file)
    val send = Intent(Intent.ACTION_SEND)
        .setType(mime)
        .putExtra(Intent.EXTRA_STREAM, uri)
        .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    try {
        context.startActivity(Intent.createChooser(send, "Share ${file.name}"))
    } catch (e: ActivityNotFoundException) {
        android.widget.Toast.makeText(context, "No app here can open ${file.name}.", android.widget.Toast.LENGTH_LONG).show()
    }
}
