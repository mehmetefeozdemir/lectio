package org.lectio.android.ui

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.ImageDecoder
import android.net.Uri
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Image
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material.icons.filled.AddCircleOutline
import androidx.compose.material.icons.filled.Slideshow
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.AssistChip
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.core.content.FileProvider
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import java.io.ByteArrayOutputStream
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.lectio.android.AppGraph
import org.lectio.android.models.SessionViewModel
import org.lectio.android.support.TimeText
import org.lectio.android.support.allLabels
import org.lectio.android.support.title
import uniffi.lectio_ffi.NoteInfo
import uniffi.lectio_ffi.NoteKind
import uniffi.lectio_ffi.NoteLabel
import uniffi.lectio_ffi.PhotoInfo
import uniffi.lectio_ffi.checkFormula

/** The lecture's notes in time order, each playing from its moment; the composer below. */
@Composable
fun NotesTab(session: SessionViewModel, graph: AppGraph, offsetNow: () -> ULong, playFrom: (ULong) -> Unit) {
    val state by session.state.collectAsStateWithLifecycle()
    val list = rememberLazyListState()
    var editing by remember { mutableStateOf<NoteInfo?>(null) }
    LaunchedEffect(state.notes.size) { if (state.notes.isNotEmpty()) list.animateScrollToItem(state.notes.size - 1) }

    Column(Modifier.fillMaxSize().imePadding()) {
        Box(Modifier.weight(1f)) {
            if (state.notes.isEmpty()) {
                Text(
                    "No notes yet. Each note you write keeps the moment of the recording it belongs to.",
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.align(Alignment.Center).padding(24.dp),
                )
            }
            LazyColumn(state = list, modifier = Modifier.fillMaxSize()) {
                items(state.notes, key = { it.id }) { note ->
                    NoteRow(note, canPlay = state.recordingPath != null, playFrom = playFrom, session = session, onEdit = { editing = note })
                    HorizontalDivider()
                }
            }
        }
        Composer(session, graph, offsetNow)
    }

    editing?.let { note ->
        var text by remember(note.id) { mutableStateOf(note.body) }
        AlertDialog(
            onDismissRequest = { editing = null },
            title = { Text("Edit note") },
            text = { OutlinedTextField(text, { text = it }, modifier = Modifier.fillMaxWidth()) },
            confirmButton = { TextButton(onClick = { session.edit(note, text); editing = null }) { Text("Save") } },
            dismissButton = { TextButton(onClick = { editing = null }) { Text("Cancel") } },
        )
    }
}

@Composable
private fun NoteRow(note: NoteInfo, canPlay: Boolean, playFrom: (ULong) -> Unit, session: SessionViewModel, onEdit: () -> Unit) {
    var menu by remember { mutableStateOf(false) }
    val state by session.state.collectAsStateWithLifecycle()
    Box {
        Row(
            Modifier.fillMaxWidth()
                .combinedClickable(onClick = {}, onLongClick = { menu = true }, onLongClickLabel = "Labels, edit and delete")
                .padding(horizontal = 16.dp, vertical = 10.dp),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            AssistChip(
                onClick = { playFrom(note.offsetMs) },
                enabled = canPlay,
                label = { Text(TimeText.clock(note.offsetMs), fontFamily = FontFamily.Monospace) },
                modifier = Modifier.testTag("note.play").semantics { contentDescription = "Play from ${TimeText.spoken(note.offsetMs)}" },
            )
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                NoteContent(note)
                if (note.labels.isNotEmpty()) {
                    Row(
                        Modifier.horizontalScroll(rememberScrollState()).semantics(mergeDescendants = true) {
                            contentDescription = "Labels: " + note.labels.joinToString(", ") { it.title }
                        },
                        horizontalArrangement = Arrangement.spacedBy(6.dp),
                    ) { note.labels.forEach { LabelChip(it) } }
                }
            }
        }
        DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
            allLabels.forEach { label ->
                DropdownMenuItem(
                    text = { Text((if (label in note.labels) "Remove " else "Add ") + label.title) },
                    leadingIcon = { Icon(label.icon, contentDescription = null, tint = label.tint) },
                    onClick = { session.toggle(label, note); menu = false },
                )
            }
            if (note.kind == NoteKind.TEXT && note.photo == null) {
                DropdownMenuItem(text = { Text("Edit") }, onClick = { menu = false; onEdit() })
            }
            val slide = state.currentSlide
            if (slide != null && note.kind != NoteKind.SLIDE) {
                DropdownMenuItem(text = { Text("Link to slide ${slide.page}") }, onClick = { session.link(note, slide); menu = false })
            }
            if (note.slide != null && note.kind != NoteKind.SLIDE) {
                DropdownMenuItem(text = { Text("Unlink from the slide") }, onClick = { session.link(note, null); menu = false })
            }
            DropdownMenuItem(text = { Text("Delete") }, onClick = { session.delete(note); menu = false })
        }
    }
}

@Composable
private fun NoteContent(note: NoteInfo) {
    val photo = note.photo
    when {
        photo != null -> {
            PhotoThumbnail(photo)
            if (note.body.isNotBlank()) Text(note.body)
        }
        note.kind == NoteKind.FORMULA -> FormulaImage(note.body)
        note.kind == NoteKind.SLIDE -> Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
            Icon(Icons.Filled.Slideshow, contentDescription = null)
            Text(note.body.ifBlank { "Slide ${note.slide?.page ?: 0u}" }, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        else -> Text(
            note.body,
            style = if (NoteLabel.HEADING in note.labels) MaterialTheme.typography.titleMedium else MaterialTheme.typography.bodyLarge,
            fontWeight = if (NoteLabel.HEADING in note.labels) FontWeight.SemiBold else null,
        )
    }
}

/** A photo note's picture, scaled down off the main thread. */
@Composable
private fun PhotoThumbnail(photo: PhotoInfo) {
    val image by produceState<ImageBitmap?>(null, photo.path) {
        value = withContext(Dispatchers.IO) { decodeScaled(photo.path, THUMBNAIL_PX)?.asImageBitmap() }
    }
    val description = if (photo.isSketch) "Sketch" else "Photo"
    image?.let {
        Image(
            it, contentDescription = description, contentScale = ContentScale.Fit,
            modifier = Modifier.fillMaxWidth().heightIn(max = 240.dp).clip(RoundedCornerShape(8.dp)),
        )
    } ?: Surface(Modifier.fillMaxWidth().heightIn(min = 120.dp).semantics { contentDescription = description }, color = MaterialTheme.colorScheme.surfaceVariant) {}
}

private const val THUMBNAIL_PX = 800

private fun decodeScaled(path: String, maxSide: Int): Bitmap? {
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeFile(path, bounds)
    var sample = 1
    while (maxOf(bounds.outWidth, bounds.outHeight) / (sample * 2) >= maxSide) sample *= 2
    return BitmapFactory.decodeFile(path, BitmapFactory.Options().apply { inSampleSize = sample })
}

/** Label chips, the add menu (formula, camera, photo library), the text field and Add. */
@Composable
private fun Composer(session: SessionViewModel, graph: AppGraph, offsetNow: () -> ULong) {
    val context = LocalContext.current
    val state by session.state.collectAsStateWithLifecycle()
    var menu by remember { mutableStateOf(false) }
    var writingFormula by remember { mutableStateOf(false) }
    // The moment a formula or photo was started, so it lands where the lecturer was.
    var startedAt by remember { mutableStateOf(0uL) }
    var cameraFile by remember { mutableStateOf<File?>(null) }

    val camera = rememberLauncherForActivityResult(ActivityResultContracts.TakePicture()) { taken ->
        val file = cameraFile
        if (taken && file != null) session.addPhoto(file.readBytes(), "", startedAt)
        file?.delete()
    }
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.PickVisualMedia()) { uri ->
        if (uri != null) jpegOrPng(context, uri)?.let { session.addPhoto(it, "", startedAt) }
    }

    Surface(tonalElevation = 3.dp) {
        Column(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                allLabels.forEach { label ->
                    val on = label in state.draftLabels
                    LabelChip(
                        label, selected = on,
                        modifier = Modifier.clip(RoundedCornerShape(50)).toggleable(value = on, role = Role.Checkbox) { session.toggleDraftLabel(label) },
                    )
                }
            }
            Row(verticalAlignment = Alignment.Bottom) {
                Box {
                    IconButton(onClick = { menu = true }) { Icon(Icons.Filled.AddCircleOutline, contentDescription = "Add a formula or photo") }
                    DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                        DropdownMenuItem(text = { Text("Formula") }, onClick = { menu = false; startedAt = offsetNow(); writingFormula = true })
                        DropdownMenuItem(text = { Text("Take a photo") }, onClick = {
                            menu = false
                            startedAt = offsetNow()
                            val file = File(graph.cameraDirectory.apply { mkdirs() }, "photo-${System.currentTimeMillis()}.jpg")
                            cameraFile = file
                            camera.launch(FileProvider.getUriForFile(context, "${context.packageName}.files", file))
                        })
                        DropdownMenuItem(text = { Text("Choose a photo") }, onClick = {
                            menu = false
                            startedAt = offsetNow()
                            picker.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly))
                        })
                    }
                }
                OutlinedTextField(
                    value = state.draft,
                    onValueChange = { session.editDraft(it, offsetNow()) },
                    placeholder = { Text("Write a note") },
                    maxLines = 5,
                    modifier = Modifier.weight(1f).testTag("composer.text"),
                )
                IconButton(
                    onClick = { session.addNote(offsetNow()) },
                    enabled = state.draft.isNotBlank(),
                    modifier = Modifier.testTag("composer.add"),
                ) { Icon(Icons.AutoMirrored.Filled.Send, contentDescription = "Add note") }
            }
        }
    }

    if (writingFormula) {
        FormulaEditor(onAdd = { latex -> session.addFormula(latex, startedAt) }) { writingFormula = false }
    }
}

/** The picked image as JPEG or PNG; other formats (HEIC) are converted, upright. */
private fun jpegOrPng(context: Context, uri: Uri): ByteArray? {
    val bytes = context.contentResolver.openInputStream(uri)?.use { it.readBytes() } ?: return null
    val jpeg = bytes.size > 2 && bytes[0] == 0xFF.toByte() && bytes[1] == 0xD8.toByte()
    val png = bytes.size > 4 && bytes[0] == 0x89.toByte() && bytes[1] == 'P'.code.toByte()
    if (jpeg || png) return bytes
    val bitmap = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
        ImageDecoder.decodeBitmap(ImageDecoder.createSource(bytes))
    } else {
        BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
    } ?: return null
    return ByteArrayOutputStream().also { bitmap.compress(Bitmap.CompressFormat.JPEG, JPEG_QUALITY, it) }.toByteArray()
}

private const val JPEG_QUALITY = 85

private val FORMULA_SNIPPETS = listOf(
    "a/b" to "\\frac{}{}", "x²" to "^{}", "xᵢ" to "_{}", "√" to "\\sqrt{}",
    "Σ" to "\\sum_{}^{}", "∫" to "\\int_{}^{}", "α" to "\\alpha", "π" to "\\pi",
)

/** LaTeX with a live preview drawn by the core, and buttons for the hard-to-type pieces. */
@Composable
private fun FormulaEditor(onAdd: (String) -> Boolean, dismiss: () -> Unit) {
    var latex by remember { mutableStateOf("") }
    val problem = if (latex.isBlank()) null else checkFormula(latex)
    AlertDialog(
        onDismissRequest = dismiss,
        title = { Text("Formula") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                OutlinedTextField(
                    latex, { latex = it },
                    label = { Text("LaTeX") }, placeholder = { Text("e.g. E = mc^2") },
                    textStyle = MaterialTheme.typography.bodyLarge.copy(fontFamily = FontFamily.Monospace),
                    modifier = Modifier.fillMaxWidth().testTag("formula.text"),
                )
                Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    FORMULA_SNIPPETS.forEach { (label, insert) ->
                        OutlinedButton(onClick = { latex += insert }, modifier = Modifier.semantics { contentDescription = "Insert $insert" }) { Text(label) }
                    }
                }
                Text("Preview", style = MaterialTheme.typography.labelLarge)
                if (latex.isBlank()) Text("Type a formula to see it here.", color = MaterialTheme.colorScheme.onSurfaceVariant) else FormulaImage(latex)
            }
        },
        confirmButton = {
            TextButton(onClick = { if (onAdd(latex)) dismiss() }, enabled = latex.isNotBlank() && problem == null) { Text("Add") }
        },
        dismissButton = { TextButton(onClick = dismiss) { Text("Cancel") } },
    )
}
