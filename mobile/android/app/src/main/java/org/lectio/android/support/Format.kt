package org.lectio.android.support

import java.util.Locale
import uniffi.lectio_ffi.LectioException
import uniffi.lectio_ffi.NoteLabel

/** The message to show for an error; the Rust core's already reads as a sentence. */
fun errorText(error: Throwable): String = when (error) {
    is LectioException -> error.message ?: "Something went wrong."
    else -> error.message ?: error.javaClass.simpleName
}

/** Milliseconds since 1970, the clock the core's cards and quizzes use. */
fun nowUnixMs(): ULong = System.currentTimeMillis().toULong()

object TimeText {
    /** "0:07", "12:30", "1:02:03". */
    fun clock(ms: ULong): String {
        val seconds = (ms / 1000u).toLong()
        val hours = seconds / 3600
        val minutes = (seconds % 3600) / 60
        val rest = seconds % 60
        val ss = rest.toString().padStart(2, '0')
        return if (hours > 0) "$hours:${minutes.toString().padStart(2, '0')}:$ss" else "$minutes:$ss"
    }

    /** For TalkBack: "1 minute, 5 seconds". */
    fun spoken(ms: ULong): String {
        val seconds = (ms / 1000u).toLong()
        val parts = listOf(seconds / 3600 to "hour", (seconds % 3600) / 60 to "minute", seconds % 60 to "second")
            .filter { (count, _) -> count > 0 }
            .map { (count, unit) -> "$count $unit${if (count == 1L) "" else "s"}" }
        return if (parts.isEmpty()) "0 seconds" else parts.joinToString(", ")
    }
}

/** The five labels in the order the desktop shows them. */
val allLabels: List<NoteLabel> = listOf(NoteLabel.IMPORTANT, NoteLabel.REVIEW, NoteLabel.TASK, NoteLabel.DEFINITION, NoteLabel.HEADING)

val NoteLabel.title: String
    get() = when (this) {
        NoteLabel.IMPORTANT -> "Important"
        NoteLabel.REVIEW -> "Review"
        NoteLabel.TASK -> "Task"
        NoteLabel.DEFINITION -> "Definition"
        NoteLabel.HEADING -> "Heading"
    }

/** "840 B", "34 KB", "2.1 MB" (powers of 1000, as file managers show). */
fun fileSize(bytes: Long): String = when {
    bytes < 1_000 -> "$bytes B"
    bytes < 1_000_000 -> "${(bytes + 500) / 1_000} KB"
    else -> String.format(Locale.getDefault(), "%.1f MB", bytes / 1_000_000.0)
}

/**
 * The line shown when a transcription finishes, the same as on iOS:
 * "Done: 7 segments in 12.3 s (2.1× real time)." Above 1× is faster than
 * the audio plays. Without the audio's length, the speed is left out.
 */
fun transcriptionDoneText(segments: UInt, seconds: Double, audioMs: ULong?): String {
    val taken = String.format(Locale.getDefault(), "%.1f", seconds)
    if (audioMs == null || audioMs == 0uL || seconds <= 0.0) return "Done: $segments segments in $taken s."
    val speed = String.format(Locale.getDefault(), "%.1f", audioMs.toDouble() / 1000.0 / seconds)
    return "Done: $segments segments in $taken s ($speed× real time)."
}
