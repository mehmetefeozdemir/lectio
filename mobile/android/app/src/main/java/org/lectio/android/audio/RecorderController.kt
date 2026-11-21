package org.lectio.android.audio

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import org.lectio.android.support.errorText
import uniffi.lectio_ffi.AudioRecorder
import uniffi.lectio_ffi.LectioLibrary

/**
 * Records one lecture at a time (the same rules as iOS, ADR-031). Samples go
 * from the audio thread straight into Rust, which writes the desktop's WAV;
 * the elapsed time shown, and stamped on new notes, is what Rust has written,
 * so a pause for a phone call never leaves a gap between notes and audio.
 *
 * Interruptions: losing audio focus (a call) pauses reading; the recording
 * carries on in the same file when focus comes back, or when the user taps
 * Resume. The foreground service (see [RecordingHost]) keeps it going with the
 * screen off.
 */
class RecorderController(
    private val library: LectioLibrary,
    private val makeSource: () -> AudioSource,
    private val host: RecordingHost,
    private val scope: CoroutineScope,
) {
    enum class Phase { IDLE, RECORDING, INTERRUPTED }

    data class State(
        val phase: Phase = Phase.IDLE,
        val sessionId: String? = null,
        val elapsedMs: ULong = 0uL,
        val inputName: String = "",
        val problem: String? = null,
    )

    private val _state = MutableStateFlow(State())
    val state: StateFlow<State> = _state.asStateFlow()

    /** Called with the session id after a recording is saved. */
    var onFinished: ((String) -> Unit)? = null

    private val failures = FailureBox()
    private var source: AudioSource? = null
    private var recorder: AudioRecorder? = null
    private var ticker: Job? = null

    fun isRecording(session: String): Boolean = _state.value.let { it.sessionId == session && it.phase != Phase.IDLE }

    /**
     * Starts recording [session]. A lecture keeps one recording, so a session
     * that already has one is refused rather than recorded over.
     */
    fun start(session: String): Boolean {
        if (_state.value.phase != Phase.IDLE) {
            return refuse("Lectio is already recording another lecture. Stop it first.")
        }
        return try {
            val detail = library.session(session)
            if (detail.recordingPath != null) return refuse("This lecture already has a recording.")
            val recorder = AudioRecorder.start(library.recordingPath(session))
            val source = makeSource()
            val failures = this.failures
            source.start { samples, sampleRate, channels ->
                try {
                    recorder.pushF32le(samples.toLittleEndianBytes(), sampleRate.toUInt(), channels.toUInt())
                } catch (e: Exception) {
                    failures.record(errorText(e))
                }
            }
            this.recorder = recorder
            this.source = source
            _state.value = State(Phase.RECORDING, session, 0uL, source.inputName)
            host.recordingStarted(detail.info.title)
            startTicking()
            true
        } catch (e: Exception) {
            refuse(errorText(e))
        }
    }

    /** Stops and saves; returns the recording's length. */
    fun stop(): ULong? {
        val recorder = recorder ?: return null
        val session = _state.value.sessionId ?: return null
        source?.stop()
        return try {
            val duration = recorder.finish()
            library.attachRecording(session, duration)
            reset(elapsed = duration)
            onFinished?.invoke(session)
            duration
        } catch (e: Exception) {
            reset(elapsed = _state.value.elapsedMs)
            _state.update { it.copy(problem = errorText(e)) }
            null
        } finally {
            recorder.close()
        }
    }

    /** A call (or another app) took the microphone. */
    fun interruptionBegan() {
        if (_state.value.phase != Phase.RECORDING) return
        source?.pause()
        _state.update { it.copy(phase = Phase.INTERRUPTED) }
    }

    /** The interruption is over; [shouldResume] when the system gave focus back. */
    fun interruptionEnded(shouldResume: Boolean) {
        if (_state.value.phase == Phase.INTERRUPTED && shouldResume) resume()
    }

    /** Carries on in the same file after an interruption. */
    fun resume() {
        if (_state.value.phase != Phase.INTERRUPTED) return
        source?.resume()
        _state.update { it.copy(phase = Phase.RECORDING) }
    }

    /** Re-reads how much has been written (also called by tests). */
    fun tick() {
        val recorder = recorder ?: return
        // The input's name follows route changes (a headset plugged in or out).
        _state.update { it.copy(elapsedMs = recorder.durationMs(), inputName = source?.inputName ?: it.inputName) }
        failures.take()?.let { failure ->
            stop()
            _state.update { it.copy(problem = failure) }
        }
    }

    fun clearProblem() = _state.update { it.copy(problem = null) }

    private fun refuse(message: String): Boolean {
        _state.update { it.copy(problem = message) }
        return false
    }

    private fun startTicking() {
        ticker?.cancel()
        ticker = scope.launch {
            while (isActive) {
                delay(TICK_MS)
                tick()
            }
        }
    }

    private fun reset(elapsed: ULong) {
        ticker?.cancel()
        ticker = null
        source = null
        this.recorder = null
        failures.take()
        _state.value = State(elapsedMs = elapsed)
        host.recordingStopped()
    }

    companion object {
        const val TICK_MS = 200L
    }
}
