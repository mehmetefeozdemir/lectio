package org.lectio.android.audio

import android.Manifest
import android.annotation.SuppressLint
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageManager
import android.media.AudioAttributes
import android.media.AudioDeviceInfo
import android.media.AudioFocusRequest
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioRecord
import android.media.AudioTrack
import android.media.MediaRecorder
import android.os.Handler
import android.os.Looper
import androidx.core.content.ContextCompat
import java.util.concurrent.atomic.AtomicInteger
import kotlin.concurrent.thread

/**
 * The phone's microphone through [AudioRecord] (44.1 kHz float mono, the rate
 * every Android device must support; Rust converts it to 16 kHz). Android
 * routes the input itself when a wired headset is plugged in or out; the
 * routing listener keeps [inputName] current. Bluetooth headsets keep the
 * phone's own microphone, which hears the room (ADR-033).
 */
class MicrophoneSource(private val context: Context) : AudioSource {
    private var record: AudioRecord? = null
    private var reader: Thread? = null
    @Volatile private var running = false
    @Volatile private var paused = false
    @Volatile private var device = ""

    override val inputName: String get() = device

    override fun start(sink: SampleSink) {
        val granted = ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED
        check(granted) { "Lectio can't use the microphone. Allow it in Settings › Apps › Lectio › Permissions." }
        record = open()
        running = true
        reader = thread(name = "lectio-microphone") { readLoop(sink) }
    }

    @SuppressLint("MissingPermission") // checked in start()
    private fun open(): AudioRecord {
        val minBytes = AudioRecord.getMinBufferSize(RATE, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_FLOAT)
        check(minBytes > 0) { "No microphone is available right now." }
        val created = AudioRecord(
            MediaRecorder.AudioSource.MIC, RATE, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_FLOAT,
            maxOf(minBytes, RATE / 5 * Float.SIZE_BYTES),
        )
        if (created.state != AudioRecord.STATE_INITIALIZED) {
            created.release()
            error("No microphone is available right now.")
        }
        created.addOnRoutingChangedListener({ routing -> device = describe(routing.routedDevice) }, Handler(Looper.getMainLooper()))
        created.startRecording()
        device = describe(created.routedDevice)
        return created
    }

    private fun readLoop(sink: SampleSink) {
        val buffer = FloatArray(RATE / 10)
        while (running) {
            val current = record
            if (paused || current == null) {
                Thread.sleep(IDLE_MS)
                continue
            }
            val n = current.read(buffer, 0, buffer.size, AudioRecord.READ_BLOCKING)
            when {
                n > 0 -> sink(buffer.copyOf(n), RATE, 1)
                n == AudioRecord.ERROR_DEAD_OBJECT -> reopen()
                n < 0 -> Thread.sleep(IDLE_MS)
            }
        }
    }

    /** The audio server restarted (it happens after some route changes): open again. */
    private fun reopen() {
        record?.release()
        record = runCatching { open() }.getOrNull()
    }

    override fun pause() {
        paused = true
        runCatching { record?.stop() }
    }

    override fun resume() {
        runCatching { record?.startRecording() }
        paused = false
    }

    override fun stop() {
        running = false
        runCatching { record?.stop() }
        reader?.join(JOIN_MS)
        record?.release()
        record = null
    }

    companion object {
        const val RATE = 44_100
        private const val IDLE_MS = 20L
        private const val JOIN_MS = 1_000L

        fun describe(info: AudioDeviceInfo?): String = when (info?.type) {
            null -> ""
            AudioDeviceInfo.TYPE_BUILTIN_MIC -> "Phone microphone"
            AudioDeviceInfo.TYPE_WIRED_HEADSET -> "Wired headset"
            AudioDeviceInfo.TYPE_BLUETOOTH_SCO -> "Bluetooth headset"
            AudioDeviceInfo.TYPE_USB_DEVICE, AudioDeviceInfo.TYPE_USB_HEADSET -> "USB microphone"
            else -> info.productName?.toString().orEmpty()
        }
    }
}

/**
 * The speaker or headphones: an [AudioTrack] fed blocks from Rust on its own
 * thread. A generation number stops a thread left over from before a seek.
 */
class TrackOutput : AudioOutput {
    private val generation = AtomicInteger()
    @Volatile private var track: AudioTrack? = null

    override fun start(sampleRate: Int, pull: (Int) -> FloatArray, finished: () -> Unit) {
        stop()
        val minBytes = AudioTrack.getMinBufferSize(sampleRate, AudioFormat.CHANNEL_OUT_MONO, AudioFormat.ENCODING_PCM_FLOAT)
        val created = AudioTrack.Builder()
            .setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA).setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build())
            .setAudioFormat(AudioFormat.Builder().setSampleRate(sampleRate).setEncoding(AudioFormat.ENCODING_PCM_FLOAT).setChannelMask(AudioFormat.CHANNEL_OUT_MONO).build())
            .setBufferSizeInBytes(maxOf(minBytes, BLOCK_FRAMES * Float.SIZE_BYTES * 2))
            .setTransferMode(AudioTrack.MODE_STREAM)
            .build()
        created.play()
        track = created
        val mine = generation.incrementAndGet()
        thread(name = "lectio-playback") { writeLoop(created, mine, pull, finished) }
    }

    private fun writeLoop(track: AudioTrack, mine: Int, pull: (Int) -> FloatArray, finished: () -> Unit) {
        var written = 0L
        while (generation.get() == mine) {
            val block = pull(BLOCK_FRAMES)
            if (block.isEmpty()) {
                // Let what's queued finish playing before saying so.
                while (generation.get() == mine && (track.playbackHeadPosition.toLong() and 0xFFFF_FFFFL) < written) Thread.sleep(WAIT_MS)
                if (generation.get() == mine) finished()
                return
            }
            val n = track.write(block, 0, block.size, AudioTrack.WRITE_BLOCKING)
            if (n < 0) return
            written += n
        }
    }

    override fun stop() {
        generation.incrementAndGet()
        val old = track ?: return
        track = null
        runCatching {
            old.pause()
            old.flush()
            old.release()
        }
    }

    companion object {
        /** 128 ms at 16 kHz. */
        const val BLOCK_FRAMES = 2048
        private const val WAIT_MS = 20L
    }
}

/**
 * Audio focus, the foreground service and "headphones unplugged" around
 * recording and playback. A call takes audio focus: the recording pauses
 * (and carries on when focus returns) and playback pauses.
 */
class AndroidAudioHost(private val context: Context) : RecordingHost, PlaybackHost {
    lateinit var recorder: RecorderController
    lateinit var player: PlayerController

    private val audio = context.getSystemService(AudioManager::class.java)
    private val main = Handler(Looper.getMainLooper())
    private var recordingFocus: AudioFocusRequest? = null
    private var playbackFocus: AudioFocusRequest? = null
    private var noisyRegistered = false

    private val noisy = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            if (intent.action == AudioManager.ACTION_AUDIO_BECOMING_NOISY) player.pause()
        }
    }

    override fun recordingStarted(title: String) {
        recordingFocus = focus(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT) { change ->
            when (change) {
                AudioManager.AUDIOFOCUS_LOSS, AudioManager.AUDIOFOCUS_LOSS_TRANSIENT -> recorder.interruptionBegan()
                AudioManager.AUDIOFOCUS_GAIN -> recorder.interruptionEnded(shouldResume = true)
            }
        }
        RecordingService.start(context, title)
    }

    override fun recordingStopped() {
        recordingFocus?.let(audio::abandonAudioFocusRequest)
        recordingFocus = null
        RecordingService.stop(context)
    }

    override fun playbackStarted() {
        if (playbackFocus == null) {
            playbackFocus = focus(AudioManager.AUDIOFOCUS_GAIN) { change ->
                if (change == AudioManager.AUDIOFOCUS_LOSS || change == AudioManager.AUDIOFOCUS_LOSS_TRANSIENT) player.pause()
            }
        }
        if (!noisyRegistered) {
            ContextCompat.registerReceiver(context, noisy, IntentFilter(AudioManager.ACTION_AUDIO_BECOMING_NOISY), ContextCompat.RECEIVER_NOT_EXPORTED)
            noisyRegistered = true
        }
    }

    override fun playbackStopped() {
        playbackFocus?.let(audio::abandonAudioFocusRequest)
        playbackFocus = null
        if (noisyRegistered) {
            context.unregisterReceiver(noisy)
            noisyRegistered = false
        }
    }

    private fun focus(kind: Int, onChange: (Int) -> Unit): AudioFocusRequest {
        val request = AudioFocusRequest.Builder(kind)
            .setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA).setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build())
            .setWillPauseWhenDucked(false)
            .setOnAudioFocusChangeListener(onChange, main)
            .build()
        audio.requestAudioFocus(request)
        return request
    }
}
