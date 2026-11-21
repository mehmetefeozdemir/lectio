package org.lectio.android

import java.io.File
import java.util.Locale
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.test.StandardTestDispatcher
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.lectio.android.audio.AudioOutput
import org.lectio.android.audio.AudioSource
import org.lectio.android.audio.NoAudioHost
import org.lectio.android.audio.PlayerController
import org.lectio.android.audio.RecorderController
import org.lectio.android.audio.SampleSink
import org.lectio.android.audio.ToneSource
import org.lectio.android.support.TimeText
import org.lectio.android.support.errorText
import org.lectio.android.support.fileSize
import org.lectio.android.support.transcriptionDoneText
import uniffi.lectio_ffi.LectioException
import uniffi.lectio_ffi.LectioLibrary

/** Pushes samples when the test says so, like a microphone on demand. */
class FakeSource : AudioSource {
    var resumed = 0
        private set
    private var sink: SampleSink? = null

    override fun start(sink: SampleSink) { this.sink = sink }
    override fun pause() = Unit
    override fun resume() { resumed += 1 }
    override fun stop() { sink = null }

    /** [seconds] of a quiet signal at 48 kHz stereo, a common phone format. */
    fun push(seconds: Double, sampleRate: Int = 48_000, channels: Int = 2) {
        sink?.invoke(FloatArray((sampleRate * seconds).toInt() * channels) { 0.1f }, sampleRate, channels)
    }
}

/** Takes every block at once when the test asks. */
class DrainOutput : AudioOutput {
    private var pull: ((Int) -> FloatArray)? = null
    private var finished: (() -> Unit)? = null

    override fun start(sampleRate: Int, pull: (Int) -> FloatArray, finished: () -> Unit) {
        this.pull = pull
        this.finished = finished
    }

    override fun stop() { pull = null }

    /** Returns how many samples were played. */
    fun drain(): Int {
        var total = 0
        while (true) {
            val block = pull?.invoke(1024) ?: break
            if (block.isEmpty()) break
            total += block.size
        }
        finished?.invoke()
        return total
    }
}

/** A scope whose ticking loop never runs by itself (tests call tick()). */
private fun idleScope() = CoroutineScope(StandardTestDispatcher() + Job())

private fun recorder(library: LectioLibrary, source: FakeSource) = RecorderController(library, { source }, NoAudioHost, idleScope())

class RecorderControllerTest {
    @Test
    fun aRecordingIsSavedInTheDesktopFormatAndAttached() {
        // Arrange
        val (library, session) = makeSession()
        val source = FakeSource()
        val recorder = recorder(library, source)
        var finished: String? = null
        recorder.onFinished = { finished = it }

        // Act
        val started = recorder.start(session)
        source.push(1.5)
        recorder.tick()
        val elapsed = recorder.state.value.elapsedMs
        val duration = recorder.stop()!!

        // Assert
        assertTrue(started)
        assertEquals(1_500.0, elapsed.toDouble(), 50.0)
        assertEquals(RecorderController.Phase.IDLE, recorder.state.value.phase)
        assertEquals(session, finished)
        val detail = library.session(session)
        assertEquals(duration, detail.info.durationMs)
        val wav = File(detail.recordingPath!!).readBytes()
        assertEquals("RIFF", String(wav.copyOf(4)))
        assertEquals("mono", 1, wav[22].toInt())
        assertEquals("16 kHz", 16_000, (wav[24].toInt() and 0xFF) or ((wav[25].toInt() and 0xFF) shl 8) or ((wav[26].toInt() and 0xFF) shl 16))
        assertEquals("16-bit", 16, wav[34].toInt())
    }

    @Test
    fun anInterruptionPausesThenCarriesOnInTheSameFile() {
        val (library, session) = makeSession()
        val source = FakeSource()
        val recorder = recorder(library, source)
        recorder.start(session)
        source.push(1.0)

        recorder.interruptionBegan()
        assertEquals(RecorderController.Phase.INTERRUPTED, recorder.state.value.phase)
        recorder.interruptionEnded(shouldResume = false)
        assertEquals("waits for the user when focus wasn't given back", RecorderController.Phase.INTERRUPTED, recorder.state.value.phase)
        recorder.resume()
        assertEquals(RecorderController.Phase.RECORDING, recorder.state.value.phase)
        assertEquals(1, source.resumed)

        source.push(1.0)
        assertEquals(2_000.0, recorder.stop()!!.toDouble(), 50.0)
    }

    @Test
    fun anInputThatChangesFormatKeepsRecording() {
        val (library, session) = makeSession()
        val source = FakeSource()
        val recorder = recorder(library, source)
        recorder.start(session)
        source.push(1.0, sampleRate = 48_000, channels = 2)
        source.push(1.0, sampleRate = 44_100, channels = 1)
        assertEquals(2_000.0, recorder.stop()!!.toDouble(), 50.0)
    }

    @Test
    fun aLectureWithARecordingIsNeverRecordedOver() {
        val (library, session) = makeSession()
        val source = FakeSource()
        val recorder = recorder(library, source)
        recorder.start(session)
        source.push(0.5)
        recorder.stop()

        assertFalse(recorder.start(session))
        assertEquals("This lecture already has a recording.", recorder.state.value.problem)
    }
}

class PlayerControllerTest {
    private fun recordedSession(seconds: Double): Pair<String, String> {
        val (library, session) = makeSession()
        val source = FakeSource()
        val recorder = recorder(library, source)
        recorder.start(session)
        source.push(seconds)
        recorder.stop()
        return session to library.session(session).recordingPath!!
    }

    @Test
    fun playingFromANoteStartsThereAndRunsToTheEnd() {
        val (session, path) = recordedSession(2.0)
        val output = DrainOutput()
        val player = PlayerController(output, NoAudioHost, idleScope())

        player.play(session, path, fromMs = 1_000uL)
        assertTrue(player.state.value.isPlaying)
        assertEquals(1_000uL, player.state.value.positionMs)
        val played = output.drain()

        assertEquals("one second left at normal speed", 16_000.0, played.toDouble(), 1_600.0)
        assertFalse(player.state.value.isPlaying)
        assertEquals(player.state.value.durationMs, player.state.value.positionMs)
    }

    @Test
    fun doubleSpeedPlaysTheSameAudioInHalfTheTime() {
        val (session, path) = recordedSession(2.0)
        val output = DrainOutput()
        val player = PlayerController(output, NoAudioHost, idleScope())

        player.setSpeed(200)
        player.play(session, path, fromMs = 0uL)

        assertEquals("two seconds of audio in about one", 16_000.0, output.drain().toDouble(), 2_400.0)
    }
}

class FormatTest {
    @Test
    fun clockTimes() {
        assertEquals("0:00", TimeText.clock(0uL))
        assertEquals("0:07", TimeText.clock(7_900uL))
        assertEquals("12:30", TimeText.clock(750_000uL))
        assertEquals("1:02:03", TimeText.clock(3_723_000uL))
    }

    @Test
    fun spokenTimesForTalkBack() {
        assertEquals("0 seconds", TimeText.spoken(0uL))
        assertEquals("1 minute, 5 seconds", TimeText.spoken(65_000uL))
        assertEquals("2 hours, 1 second", TimeText.spoken(7_201_000uL))
    }

    @Test
    fun theTranscriptionLineMatchesIOS() {
        Locale.setDefault(Locale.US)
        assertEquals("Done: 7 segments in 12.3 s (2.1× real time).", transcriptionDoneText(7u, 12.3, 25_830uL))
        assertEquals("Done: 0 segments in 1.0 s.", transcriptionDoneText(0u, 1.0, null))
    }

    @Test
    fun fileSizes() {
        assertEquals("840 B", fileSize(840))
        assertEquals("34 KB", fileSize(33_700))
        assertEquals("2.1 MB", fileSize(2_100_000))
    }

    @Test
    fun errorsFromTheCoreKeepTheirSentence() {
        assertEquals("That item can't be found.", errorText(LectioException.Failed("That item can't be found.")))
    }

    @Test
    fun theToneIsStereo48kHzInTenthsOfASecond() {
        assertEquals(4_800 * 2, ToneSource().nextBlock().size)
    }
}
