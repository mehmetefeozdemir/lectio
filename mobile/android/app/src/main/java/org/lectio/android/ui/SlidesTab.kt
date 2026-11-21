package org.lectio.android.ui

import android.graphics.Bitmap
import android.graphics.Color as AndroidColor
import android.graphics.pdf.PdfRenderer
import android.os.ParcelFileDescriptor
import android.provider.OpenableColumns
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.ArrowForward
import androidx.compose.material.icons.automirrored.filled.Undo
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material3.Button
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.lectio.android.models.SessionViewModel
import org.lectio.android.support.TimeText
import org.lectio.android.support.errorText
import uniffi.lectio_ffi.DeckInfo
import uniffi.lectio_ffi.InkColor
import uniffi.lectio_ffi.InkStrokeInfo
import uniffi.lectio_ffi.InkTool
import uniffi.lectio_ffi.Point
import uniffi.lectio_ffi.SlideRef

private enum class Tool(val title: String) { VIEW("View"), PEN("Pen"), HIGHLIGHTER("Highlighter"), ERASER("Eraser") }

/** Pages are drawn this many pixels wide. */
private const val RENDER_WIDTH = 1400
/** What a new stroke looks like while it's drawn (the core stores the real width and opacity). */
private const val PEN_WIDTH = 0.003f
private const val HIGHLIGHTER_WIDTH = 0.016f
private const val HIGHLIGHTER_ALPHA = 0.38f

/**
 * The lecture's PDF slides (drawn by Android's PdfRenderer) with pen and
 * highlighter ink on top, stored by the core in page fractions so it lines
 * up at any size and on every platform, and the notes linked to the page.
 */
@Composable
fun SlidesTab(session: SessionViewModel, offsetNow: () -> ULong) {
    val context = LocalContext.current
    val state by session.state.collectAsStateWithLifecycle()
    val decks = state.detail?.decks.orEmpty()
    var deckId by remember { mutableStateOf<String?>(null) }
    var page by remember { mutableStateOf(1u) }
    val deck = decks.firstOrNull { it.id == deckId } ?: decks.firstOrNull()

    val importer = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        try {
            val name = context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
                ?.use { c -> if (c.moveToFirst()) c.getString(0) else null } ?: "slides.pdf"
            val bytes = context.contentResolver.openInputStream(uri)?.use { it.readBytes() } ?: return@rememberLauncherForActivityResult
            session.importSlides(name, bytes)?.let { deckId = it; page = 1u }
        } catch (e: Exception) {
            android.util.Log.w("Lectio", "Import failed: ${errorText(e)}")
        }
    }

    if (deck == null) {
        Column(Modifier.fillMaxSize().padding(24.dp), verticalArrangement = Arrangement.Center, horizontalAlignment = Alignment.CenterHorizontally) {
            Text("No slides yet", style = MaterialTheme.typography.titleLarge)
            Text("Import the lecture's PDF to write on it and link notes to its pages.", color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(vertical = 8.dp))
            Button(onClick = { importer.launch(arrayOf("application/pdf")) }) { Text("Import a PDF") }
        }
        return
    }
    LaunchedEffect(deck.id, page) { session.setCurrentSlide(SlideRef(deck.id, page)) }

    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = 12.dp)) {
        Pager(deck, decks, page, onPage = { page = it }, onDeck = { deckId = it; page = 1u }, onPost = {
            session.postSlide(SlideRef(deck.id, page), offsetNow())
        }, onImport = { importer.launch(arrayOf("application/pdf")) })
        InkPage(session, deck, page)
        val linked = state.notesOn(SlideRef(deck.id, page))
        if (linked.isNotEmpty()) {
            Text("Notes on this page", style = MaterialTheme.typography.labelLarge, modifier = Modifier.padding(top = 8.dp))
            linked.forEach { Text("${TimeText.clock(it.offsetMs)}  ${it.body}", maxLines = 2, style = MaterialTheme.typography.bodyMedium) }
        }
    }
}

@Composable
private fun Pager(deck: DeckInfo, decks: List<DeckInfo>, page: UInt, onPage: (UInt) -> Unit, onDeck: (String) -> Unit, onPost: () -> Unit, onImport: () -> Unit) {
    var menu by remember { mutableStateOf(false) }
    Row(verticalAlignment = Alignment.CenterVertically) {
        Text(deck.title, style = MaterialTheme.typography.titleSmall, maxLines = 1, modifier = Modifier.weight(1f))
        IconButton(onClick = { onPage(page - 1u) }, enabled = page > 1u) { Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Previous page") }
        Text("$page / ${deck.pageCount}", modifier = Modifier.semantics { contentDescription = "Page $page of ${deck.pageCount}" })
        IconButton(onClick = { onPage(page + 1u) }, enabled = page < deck.pageCount) { Icon(Icons.AutoMirrored.Filled.ArrowForward, contentDescription = "Next page") }
        Box {
            IconButton(onClick = { menu = true }) { Icon(Icons.Filled.MoreVert, contentDescription = "Slide actions") }
            DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                DropdownMenuItem(text = { Text("Add this slide to the notes") }, onClick = { menu = false; onPost() })
                decks.filter { it.id != deck.id }.forEach { other ->
                    DropdownMenuItem(text = { Text("Show ${other.title}") }, onClick = { menu = false; onDeck(other.id) })
                }
                DropdownMenuItem(text = { Text("Import another PDF") }, onClick = { menu = false; onImport() })
            }
        }
    }
}

@Composable
private fun InkPage(session: SessionViewModel, deck: DeckInfo, page: UInt) {
    var bitmap by remember { mutableStateOf<Bitmap?>(null) }
    var strokes by remember { mutableStateOf<List<InkStrokeInfo>>(emptyList()) }
    val drawing = remember { mutableStateListOf<Point>() }
    var tool by remember { mutableStateOf(Tool.VIEW) }
    var color by remember { mutableStateOf(InkColor.RED) }

    fun reloadInk() {
        strokes = runCatching { session.library.ink(session.sessionId, deck.id, page) }.getOrDefault(emptyList())
    }
    fun change(edit: () -> Unit) {
        runCatching(edit)
        reloadInk()
    }
    LaunchedEffect(deck.pdfPath, page) {
        bitmap = withContext(Dispatchers.IO) { renderPage(deck.pdfPath, page.toInt() - 1) }
        reloadInk()
    }

    val image = bitmap
    val aspect = if (image != null) image.width.toFloat() / image.height else 4f / 3f
    BoxWithConstraints(
        Modifier.fillMaxWidth().aspectRatio(aspect).semantics {
            contentDescription = "Slide $page"
            stateDescription = if (strokes.isEmpty()) "" else "${strokes.size} ink strokes"
        },
    ) {
        image?.let { Image(it.asImageBitmap(), contentDescription = null, modifier = Modifier.fillMaxSize()) }
        Canvas(
            Modifier.fillMaxSize().pointerInput(tool, color, deck.id, page) {
                if (tool == Tool.VIEW) return@pointerInput
                fun at(offset: Offset) = Point(offset.x / size.width, offset.y / size.height)
                detectDragGestures(
                    onDragStart = { start ->
                        if (tool == Tool.ERASER) change { session.library.eraseInk(session.sessionId, deck.id, page, at(start)) } else drawing.add(at(start))
                    },
                    onDrag = { pointer, _ ->
                        val point = at(pointer.position)
                        if (tool == Tool.ERASER) change { session.library.eraseInk(session.sessionId, deck.id, page, point) } else drawing.add(point)
                    },
                    onDragEnd = {
                        val points = drawing.toList()
                        drawing.clear()
                        if (points.isNotEmpty() && tool != Tool.ERASER) {
                            val inkTool = if (tool == Tool.PEN) InkTool.PEN else InkTool.HIGHLIGHTER
                            change { session.library.addInkStroke(session.sessionId, deck.id, page, inkTool, color, points) }
                        }
                    },
                )
            },
        ) {
            strokes.forEach { drawStroke(it.points, it.drawnWidth, it.color.color.copy(alpha = it.opacity)) }
            if (drawing.isNotEmpty()) {
                val highlight = tool == Tool.HIGHLIGHTER
                drawStroke(drawing, if (highlight) HIGHLIGHTER_WIDTH else PEN_WIDTH, color.color.copy(alpha = if (highlight) HIGHLIGHTER_ALPHA else 1f))
            }
        }
    }

    Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(vertical = 4.dp)) {
        SingleChoiceSegmentedButtonRow(Modifier.weight(1f)) {
            Tool.entries.forEachIndexed { i, t ->
                SegmentedButton(selected = tool == t, onClick = { tool = t }, shape = SegmentedButtonDefaults.itemShape(i, Tool.entries.size)) {
                    Text(t.title, maxLines = 1, style = MaterialTheme.typography.labelSmall)
                }
            }
        }
    }
    Row(verticalAlignment = Alignment.CenterVertically) {
        if (tool == Tool.PEN || tool == Tool.HIGHLIGHTER) {
            allInkColors.forEach { c ->
                IconButton(onClick = { color = c }, modifier = Modifier.semantics { contentDescription = "${c.title} ink"; stateDescription = if (c == color) "Chosen" else "" }) {
                    Surface(Modifier.size(if (c == color) 28.dp else 20.dp), shape = CircleShape, color = c.color) {}
                }
            }
        }
        Box(Modifier.weight(1f))
        IconButton(onClick = { change { session.library.undoInk(session.sessionId, deck.id, page) } }, enabled = strokes.isNotEmpty()) {
            Icon(Icons.AutoMirrored.Filled.Undo, contentDescription = "Undo ink")
        }
        IconButton(onClick = { change { session.library.clearInk(session.sessionId, deck.id, page) } }, enabled = strokes.isNotEmpty()) {
            Icon(Icons.Filled.Delete, contentDescription = "Clear ink on this page")
        }
    }
}

private fun DrawScope.drawStroke(points: List<Point>, width: Float, color: Color) {
    if (points.isEmpty()) return
    val path = Path().apply {
        moveTo(points[0].x * size.width, points[0].y * size.height)
        points.drop(1).forEach { lineTo(it.x * size.width, it.y * size.height) }
        if (points.size == 1) lineTo(points[0].x * size.width + 0.5f, points[0].y * size.height)
    }
    drawPath(path, color, style = Stroke(width = width * size.width, cap = StrokeCap.Round, join = StrokeJoin.Round))
}

/** One page as a white-backed bitmap, [RENDER_WIDTH] wide. */
private fun renderPage(pdfPath: String, index: Int): Bitmap? = runCatching {
    ParcelFileDescriptor.open(File(pdfPath), ParcelFileDescriptor.MODE_READ_ONLY).use { fd ->
        PdfRenderer(fd).use { renderer ->
            if (index !in 0 until renderer.pageCount) return@use null
            renderer.openPage(index).use { pdfPage ->
                val height = (RENDER_WIDTH.toFloat() * pdfPage.height / pdfPage.width).toInt().coerceAtLeast(1)
                Bitmap.createBitmap(RENDER_WIDTH, height, Bitmap.Config.ARGB_8888).also {
                    it.eraseColor(AndroidColor.WHITE)
                    pdfPage.render(it, null, null, PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY)
                }
            }
        }
    }
}.getOrNull()
