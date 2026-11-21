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
import uniffi.lectio_ffi.AudioPlayer

/**
 * Plays a session's recording at 50–200 % speed, from any moment. Rust's
 * [AudioPlayer] makes the blocks (the desktop's WSOLA time-stretch); the
 * [AudioOutput] plays them.
 */
class PlayerController(
    private val output: AudioOutput,
    private val host: PlaybackHost,
    private val scope: CoroutineScope,
) {
    data class State(
        val sessionId: String? = null,
        val isPlaying: Boolean = false,
        val positionMs: ULong = 0uL,
        val durationMs: ULong = 0uL,
        val speedPercent: Int = 100,
        val problem: String? = null,
    )

    private val _state = MutableStateFlow(State())
    val state: StateFlow<State> = _state.asStateFlow()

    private var player: AudioPlayer? = null
    private var ticker: Job? = null

    fun isLoaded(session: String): Boolean = _state.value.sessionId == session && player != null

    /** Opens a session's recording (without playing), unless another one is playing. */
    fun prepare(session: String, path: String) {
        if (isLoaded(session) || _state.value.isPlaying) return
        try {
            val opened = AudioPlayer.open(path)
            opened.setSpeedPercent(_state.value.speedPercent.toUShort())
            player?.close()
            player = opened
            _state.update { it.copy(sessionId = session, durationMs = opened.durationMs(), positionMs = 0uL) }
        } catch (e: Exception) {
            _state.update { it.copy(problem = errorText(e)) }
        }
    }

    /** Plays [session] from [fromMs] (a note's moment), or from where it was. */
    fun play(session: String, path: String, fromMs: ULong? = null) {
        if (!isLoaded(session)) {
            stopOutput()
            player?.close()
            player = null
            _state.update { it.copy(sessionId = null) }
            prepare(session, path)
        }
        startOutput(fromMs)
    }

    fun pause() {
        stopOutput()
        tick()
    }

    fun seek(toMs: ULong) {
        val player = player ?: return
        if (_state.value.isPlaying) {
            // Restarting the output drops the blocks queued from the old position.
            startOutput(toMs)
        } else {
            player.seekMs(toMs)
            _state.update { it.copy(positionMs = toMs) }
        }
    }

    fun setSpeed(percent: Int) {
        _state.update { it.copy(speedPercent = percent) }
        player?.setSpeedPercent(percent.toUShort())
    }

    fun tick() {
        val player = player ?: return
        _state.update { it.copy(positionMs = minOf(player.positionMs(), it.durationMs)) }
    }

    fun clearProblem() = _state.update { it.copy(problem = null) }

    private fun startOutput(fromMs: ULong?) {
        val player = player ?: return
        output.stop()
        if (fromMs != null) {
            player.seekMs(fromMs)
            _state.update { it.copy(positionMs = fromMs) }
        } else if (player.isFinished()) {
            player.seekMs(0uL)
        }
        try {
            output.start(
                sampleRate = player.sampleRate().toInt(),
                pull = { frames -> player.nextBlockF32le(frames.toUInt()).littleEndianFloats() },
                finished = ::reachedEnd,
            )
            host.playbackStarted()
            _state.update { it.copy(isPlaying = true) }
            startTicking()
        } catch (e: Exception) {
            _state.update { it.copy(problem = "Lectio can't play the recording: ${errorText(e)}") }
        }
    }

    private fun reachedEnd() {
        stopOutput()
        _state.update { it.copy(positionMs = it.durationMs) }
    }

    private fun stopOutput() {
        output.stop()
        ticker?.cancel()
        ticker = null
        if (_state.value.isPlaying) host.playbackStopped()
        _state.update { it.copy(isPlaying = false) }
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

    companion object {
        val SPEEDS = listOf(50, 75, 100, 125, 150, 175, 200)
        const val TICK_MS = 100L
    }
}
