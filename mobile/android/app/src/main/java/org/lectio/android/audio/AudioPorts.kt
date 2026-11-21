package org.lectio.android.audio

import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.Executors
import java.util.concurrent.ScheduledExecutorService
import java.util.concurrent.TimeUnit
import kotlin.math.PI
import kotlin.math.sin

/**
 * Interleaved samples (-1..1) as the input delivered them, with its rate and
 * channel count; Rust converts them to 16 kHz mono.
 */
typealias SampleSink = (samples: FloatArray, sampleRate: Int, channels: Int) -> Unit

/**
 * Samples as little-endian 32-bit floats, for the Rust calls that take bytes:
 * JNA copies a byte array in one go, but a `List<Float>` one boxed number at
 * a time, which can fall behind the microphone (PROBLEMS P-063).
 */
fun FloatArray.toLittleEndianBytes(): ByteArray =
    ByteBuffer.allocate(size * Float.SIZE_BYTES).order(ByteOrder.LITTLE_ENDIAN).also { it.asFloatBuffer().put(this) }.array()

fun ByteArray.littleEndianFloats(): FloatArray =
    FloatArray(size / Float.SIZE_BYTES).also { ByteBuffer.wrap(this).order(ByteOrder.LITTLE_ENDIAN).asFloatBuffer().get(it) }

/** Where recorded sound comes from: the microphone, or a test tone. */
interface AudioSource {
    /** Starts delivering samples to [sink], on its own thread. */
    fun start(sink: SampleSink)
    /** Stops reading for a while (a phone call took the microphone). */
    fun pause()
    fun resume()
    fun stop()
    /** "Built-in microphone", "Wired headset", ... (empty if unknown). */
    val inputName: String get() = ""
}

/**
 * Where played sound goes. [pull] returns the next mono block at
 * [sampleRate], empty at the end; [finished] runs once everything queued has
 * been played.
 */
interface AudioOutput {
    fun start(sampleRate: Int, pull: (frames: Int) -> FloatArray, finished: () -> Unit)
    fun stop()
}

/**
 * What the app does around a recording or playback: on Android, audio focus,
 * the foreground service and its notification, the "headphones unplugged"
 * broadcast. Tests use [NoAudioHost].
 */
interface RecordingHost {
    fun recordingStarted(title: String)
    fun recordingStopped()
}

interface PlaybackHost {
    fun playbackStarted()
    fun playbackStopped()
}

object NoAudioHost : RecordingHost, PlaybackHost {
    override fun recordingStarted(title: String) = Unit
    override fun recordingStopped() = Unit
    override fun playbackStarted() = Unit
    override fun playbackStopped() = Unit
}

/** A failure seen on an audio thread, picked up on the main thread. */
class FailureBox {
    private var message: String? = null

    @Synchronized
    fun record(text: String) {
        if (message == null) message = text
    }

    @Synchronized
    fun take(): String? = message.also { message = null }
}

/**
 * A steady 440 Hz tone, 48 kHz stereo, in 100 ms buffers: for UI tests and
 * emulators without a microphone. Stereo 48 kHz on purpose, so Rust still
 * converts it to 16 kHz mono like a real input.
 */
class ToneSource : AudioSource {
    private var timer: ScheduledExecutorService? = null
    private var phase = 0.0
    @Volatile private var paused = false

    override val inputName: String get() = "Test tone"

    override fun start(sink: SampleSink) {
        val executor = Executors.newSingleThreadScheduledExecutor()
        executor.scheduleAtFixedRate({ if (!paused) sink(nextBlock(), SAMPLE_RATE, CHANNELS) }, 0, BLOCK_MS, TimeUnit.MILLISECONDS)
        timer = executor
    }

    override fun pause() { paused = true }
    override fun resume() { paused = false }

    override fun stop() {
        timer?.shutdownNow()
        timer = null
    }

    /** One block of the tone, the same sample in both channels. */
    fun nextBlock(): FloatArray {
        val frames = SAMPLE_RATE * BLOCK_MS.toInt() / 1000
        val step = 2 * PI * FREQUENCY / SAMPLE_RATE
        val samples = FloatArray(frames * CHANNELS)
        for (frame in 0 until frames) {
            val value = (LEVEL * sin(phase)).toFloat()
            phase = (phase + step) % (2 * PI)
            for (channel in 0 until CHANNELS) samples[frame * CHANNELS + channel] = value
        }
        return samples
    }

    companion object {
        const val SAMPLE_RATE = 48_000
        const val CHANNELS = 2
        const val BLOCK_MS = 100L
        const val FREQUENCY = 440.0
        const val LEVEL = 0.25
    }
}

/**
 * Plays nothing but takes blocks at the real-time pace: for UI tests on
 * emulators without a sound device.
 */
class ClockOutput : AudioOutput {
    private var timer: ScheduledExecutorService? = null

    override fun start(sampleRate: Int, pull: (Int) -> FloatArray, finished: () -> Unit) {
        stop()
        val frames = sampleRate * BLOCK_MS.toInt() / 1000
        val executor = Executors.newSingleThreadScheduledExecutor()
        executor.scheduleAtFixedRate({
            if (pull(frames).isEmpty()) {
                finished()
                executor.shutdown()
            }
        }, 0, BLOCK_MS, TimeUnit.MILLISECONDS)
        timer = executor
    }

    override fun stop() {
        timer?.shutdownNow()
        timer = null
    }

    companion object {
        const val BLOCK_MS = 100L
    }
}
