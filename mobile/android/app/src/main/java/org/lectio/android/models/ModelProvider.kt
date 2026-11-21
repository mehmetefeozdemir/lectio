package org.lectio.android.models

import uniffi.lectio_ffi.ModelInfo
import uniffi.lectio_ffi.ProgressListener
import uniffi.lectio_ffi.SpeechEngine
import uniffi.lectio_ffi.StudyEngine

/**
 * The on-device models (Whisper for transcripts, the study model for
 * outlines and quizzes): files kept in `models/<folder>/` in the desktop's
 * layout, downloaded once and checked by the core (size and SHA-256), and
 * loaded once per launch. [ModelStore] is the Android implementation.
 */
interface ModelProvider {
    fun isDownloaded(model: ModelInfo): Boolean

    /** Downloads the missing files; [progress] gets 0..1 over all of them. */
    suspend fun download(model: ModelInfo, progress: (Double) -> Unit)

    suspend fun speechEngine(model: ModelInfo): SpeechEngine

    suspend fun studyEngine(model: ModelInfo): StudyEngine
}

/** Progress (0..1) from a Rust engine, and a way to stop it between steps. */
class JobProgress(private val report: (Double) -> Unit) : ProgressListener {
    @Volatile
    private var cancelled = false

    override fun progress(done: ULong, total: ULong) {
        report(if (total == 0uL) 0.0 else minOf(1.0, done.toDouble() / total.toDouble()))
    }

    override fun shouldContinue(): Boolean = !cancelled

    fun cancel() {
        cancelled = true
    }
}
