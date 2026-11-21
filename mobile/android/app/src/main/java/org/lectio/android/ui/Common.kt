package org.lectio.android.ui

import android.graphics.BitmapFactory
import androidx.compose.foundation.Image
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.MenuBook
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Replay
import androidx.compose.material.icons.filled.Star
import androidx.compose.material.icons.filled.Title
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.lectio.android.support.errorText
import org.lectio.android.support.title
import uniffi.lectio_ffi.InkColor
import uniffi.lectio_ffi.NoteLabel
import uniffi.lectio_ffi.checkFormula
import uniffi.lectio_ffi.inkRgb
import uniffi.lectio_ffi.renderFormulaPng

val NoteLabel.icon: ImageVector
    get() = when (this) {
        NoteLabel.IMPORTANT -> Icons.Filled.Star
        NoteLabel.REVIEW -> Icons.Filled.Replay
        NoteLabel.TASK -> Icons.Filled.CheckCircle
        NoteLabel.DEFINITION -> Icons.AutoMirrored.Filled.MenuBook
        NoteLabel.HEADING -> Icons.Filled.Title
    }

/** The label colours of the iOS app (orange, purple, green, teal, blue). */
val NoteLabel.tint: Color
    get() = when (this) {
        NoteLabel.IMPORTANT -> Color(0xFFE8890C)
        NoteLabel.REVIEW -> Color(0xFF9B51E0)
        NoteLabel.TASK -> Color(0xFF2E9E4F)
        NoteLabel.DEFINITION -> Color(0xFF14A3A3)
        NoteLabel.HEADING -> Color(0xFF3B7BE8)
    }

/** The same sRGB as the desktop, iOS and the exports (from the core). */
val InkColor.color: Color
    get() = inkRgb(this).let { rgb -> if (rgb.size == 3) Color(rgb[0].toInt() and 0xFF, rgb[1].toInt() and 0xFF, rgb[2].toInt() and 0xFF) else Color.Black }

val allInkColors = listOf(InkColor.BLACK, InkColor.RED, InkColor.BLUE, InkColor.GREEN, InkColor.YELLOW)

val InkColor.title: String
    get() = name.lowercase().replaceFirstChar { it.uppercase() }

/** A label chip, in the feed and (selectable) in the composer. */
@Composable
fun LabelChip(label: NoteLabel, selected: Boolean = true, modifier: Modifier = Modifier) {
    Surface(
        modifier = modifier,
        shape = RoundedCornerShape(50),
        color = if (selected) label.tint.copy(alpha = 0.16f) else MaterialTheme.colorScheme.surfaceVariant,
        contentColor = if (selected) label.tint else MaterialTheme.colorScheme.onSurfaceVariant,
    ) {
        Row(Modifier.padding(horizontal = 8.dp, vertical = 4.dp), horizontalArrangement = Arrangement.spacedBy(4.dp), verticalAlignment = Alignment.CenterVertically) {
            Icon(label.icon, contentDescription = null, modifier = Modifier.size(16.dp))
            Text(label.title, style = MaterialTheme.typography.labelMedium)
        }
    }
}

/** "Something went wrong" with the core's sentence. */
@Composable
fun ProblemDialog(message: String?, dismiss: () -> Unit) {
    if (message == null) return
    AlertDialog(
        onDismissRequest = dismiss,
        confirmButton = { TextButton(onClick = dismiss) { Text("OK") } },
        title = { Text("Something went wrong") },
        text = { Text(message) },
    )
}

private sealed interface FormulaPicture {
    data object Drawing : FormulaPicture
    data class Ready(val image: ImageBitmap) : FormulaPicture
    data class Problem(val message: String) : FormulaPicture
}

/**
 * A formula drawn by the core (the same renderer as the desktop, iOS and the
 * exports), in light or dark ink, at the reader's text size (so it grows
 * with the system font size). TalkBack reads its LaTeX.
 */
@Composable
fun FormulaImage(latex: String, modifier: Modifier = Modifier) {
    val dark = isSystemInDarkTheme()
    val sizePx = with(LocalDensity.current) { 20.sp.toPx() }
    val picture by produceState<FormulaPicture>(FormulaPicture.Drawing, latex, dark, sizePx) {
        value = withContext(Dispatchers.Default) { draw(latex, sizePx, dark) }
    }
    val described = modifier.semantics { contentDescription = "Formula: $latex" }
    when (val p = picture) {
        is FormulaPicture.Ready -> Image(p.image, contentDescription = null, modifier = described)
        is FormulaPicture.Problem -> Text(p.message, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall, modifier = described)
        FormulaPicture.Drawing -> Text(latex, fontSize = 14.sp, modifier = described)
    }
}

private fun draw(latex: String, sizePx: Float, dark: Boolean): FormulaPicture {
    checkFormula(latex)?.let { return FormulaPicture.Problem(it) }
    return try {
        val png = renderFormulaPng(latex, sizePx, dark)
        BitmapFactory.decodeByteArray(png, 0, png.size)?.let { FormulaPicture.Ready(it.asImageBitmap()) }
            ?: FormulaPicture.Problem("The formula couldn't be drawn.")
    } catch (e: Exception) {
        FormulaPicture.Problem(errorText(e))
    }
}
