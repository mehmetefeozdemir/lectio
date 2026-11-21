package org.lectio.android

import android.content.Context
import android.util.Log
import androidx.activity.ComponentActivity
import androidx.compose.ui.test.DeviceConfigurationOverride
import androidx.compose.ui.test.DarkMode
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.FontScale
import androidx.compose.ui.test.ForcedSize
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.then
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.services.storage.TestStorage
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import androidx.test.rule.GrantPermissionRule
import org.junit.runner.RunWith
import org.lectio.android.audio.MicrophoneSource
import org.lectio.android.audio.ToneSource
import org.lectio.android.audio.toLittleEndianBytes
import org.lectio.android.ui.LectioRoot
import org.lectio.android.ui.theme.LectioTheme
import uniffi.lectio_ffi.AudioRecorder
import uniffi.lectio_ffi.ExportFormat
import uniffi.lectio_ffi.LectioLibrary
import uniffi.lectio_ffi.NoteLabel
import uniffi.lectio_ffi.SpeechEngine

/** At 200 % text in dark mode, the home list and a lecture still show their content. */
@OptIn(ExperimentalTestApi::class)
@RunWith(AndroidJUnit4::class)
class LargeTextDarkModeTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun homeAndALectureReadAt200PercentInDarkMode() {
        val app = ApplicationProvider.getApplicationContext<LectioApplication>()
        app.startForTests()
        val graph = app.graph!!
        val session = graph.library.addSession(graph.library.addCourse("Biology"), "Cells and energy")
        graph.library.addNote(session, 2_000uL, "Mitochondria make ATP", listOf(NoteLabel.IMPORTANT))
        graph.library.addFormulaNote(session, 9_000uL, "E = mc^2", emptyList())

        compose.setContent {
            DeviceConfigurationOverride(DeviceConfigurationOverride.FontScale(2f) then DeviceConfigurationOverride.DarkMode(true)) {
                LectioTheme { LectioRoot(graph) }
            }
        }
        compose.onNodeWithText("Cells and energy").assertIsDisplayed()
        compose.waitForIdle()
        screenshot("5-home-200-percent-dark")
        compose.onNodeWithText("Cells and energy").performClick()
        compose.waitUntilExactlyOneExists(hasTestTag("audio.record"), 10_000)
        compose.onNodeWithText("Mitochondria make ATP").assertIsDisplayed()
        compose.onNodeWithTag("audio.record").assertIsDisplayed()
        compose.waitForIdle()
        screenshot("6-lecture-200-percent-dark")
    }
}

/** On a tablet-sized window the list and the open lecture sit side by side. */
@OptIn(ExperimentalTestApi::class)
@RunWith(AndroidJUnit4::class)
class TwoPaneTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun aTabletShowsTheListAndTheLectureTogether() {
        val app = ApplicationProvider.getApplicationContext<LectioApplication>()
        app.startForTests()
        val graph = app.graph!!
        val course = graph.library.addCourse("Biology")
        graph.library.addSession(course, "Photosynthesis")
        val cells = graph.library.addSession(course, "Cells")
        graph.library.addNote(cells, 4_000uL, "Mitochondria make ATP", listOf(NoteLabel.DEFINITION))

        compose.setContent {
            DeviceConfigurationOverride(DeviceConfigurationOverride.ForcedSize(DpSize(900.dp, 700.dp))) {
                LectioTheme { LectioRoot(graph) }
            }
        }
        compose.onNodeWithText("Choose a lecture, or start a new one.").assertIsDisplayed()
        compose.onNodeWithText("Cells").performClick()
        compose.waitUntilExactlyOneExists(hasTestTag("audio.record"), 10_000)
        // Both panes at once: the list (with the other lecture) and the lecture's notes.
        compose.onNodeWithText("Photosynthesis").assertIsDisplayed()
        compose.onNodeWithText("Mitochondria make ATP").assertIsDisplayed()
        compose.waitForIdle()
        val image = compose.onRoot().captureToImage().asAndroidBitmap()
        TestStorage().openOutputFile("screenshots/7-tablet-two-pane.png").use { image.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }
    }
}

/** The real capture path: AudioRecord at 44.1 kHz float, delivered on its own thread. */
@RunWith(AndroidJUnit4::class)
class MicrophoneTest {
    @get:Rule val microphone: GrantPermissionRule = GrantPermissionRule.grant(android.Manifest.permission.RECORD_AUDIO)

    @Test
    fun theMicrophoneDeliversAboutOneSecondInOneSecond() {
        val source = MicrophoneSource(ApplicationProvider.getApplicationContext())
        val received = java.util.concurrent.atomic.AtomicLong()
        val started = runCatching { source.start { samples, rate, channels -> assertEquals(MicrophoneSource.RATE, rate); assertEquals(1, channels); received.addAndGet(samples.size.toLong()) } }
        assumeTrue("no microphone on this machine: ${started.exceptionOrNull()?.message}", started.isSuccess)
        Thread.sleep(1_000)
        source.stop()
        Log.i("LectioTiming", "microphone: ${received.get()} samples in 1 s (${source.inputName})")
        assertEquals(MicrophoneSource.RATE.toDouble(), received.get().toDouble(), MicrophoneSource.RATE * 0.25)
    }
}

/** The Rust library for this device's ABI loads and keeps the desktop's formats. */
@RunWith(AndroidJUnit4::class)
class RustOnDeviceTest {
    private val context: Context get() = ApplicationProvider.getApplicationContext()

    @Test
    fun aLibraryRecordingAndExportWorkOnTheDevice() {
        val folder = File(context.cacheDir, "rust-on-device").apply { deleteRecursively(); mkdirs() }
        val library = LectioLibrary.open(folder.path)
        val session = library.addSession(library.addCourse("Biology"), "Cells")
        library.addNote(session, 1_500uL, "BFS uses a queue", listOf(NoteLabel.REVIEW))

        val recorder = AudioRecorder.start(library.recordingPath(session))
        recorder.push(List(48_000 * 2) { 0.1f }, 48_000u, 2u)
        val duration = recorder.finish()
        library.attachRecording(session, duration)

        assertEquals(1_000.0, duration.toDouble(), 50.0)
        assertTrue(File(folder, "library.json").exists())
        assertTrue(File(folder, "recordings/$session.wav").exists())
        val pdf = library.export(session, ExportFormat.PDF, false)
        assertEquals("%PDF", String(pdf.copyOf(4)))
        assertFalse(library.session(session).notes.isEmpty())
    }

    /** The UI test's "record 3 s" depends on the tone arriving in real time. */
    @Test
    fun theTestToneRecordsInRealTime() {
        val folder = File(context.cacheDir, "tone-timing").apply { deleteRecursively(); mkdirs() }
        val recorder = AudioRecorder.start(File(folder, "tone.wav").path)
        val tone = ToneSource()
        val pushMs = mutableListOf<Long>()
        tone.start { samples, rate, channels ->
            val started = System.nanoTime()
            recorder.pushF32le(samples.toLittleEndianBytes(), rate.toUInt(), channels.toUInt())
            synchronized(pushMs) { pushMs += (System.nanoTime() - started) / 1_000_000 }
        }
        Thread.sleep(TONE_MS)
        tone.stop()
        val duration = recorder.finish().toLong()
        val slowest = synchronized(pushMs) { pushMs.maxOrNull() ?: 0 }
        Log.i("LectioTiming", "tone: $duration ms recorded in $TONE_MS ms; ${pushMs.size} pushes, slowest $slowest ms")
        assertEquals("recorded $duration ms in $TONE_MS ms (slowest push $slowest ms)", TONE_MS.toDouble(), duration.toDouble(), TONE_MS * 0.1)
    }

    private companion object {
        const val TONE_MS = 2_000L
    }
}

/**
 * Times Whisper tiny and base through the app's own Kotlin → Rust path.
 * Skipped unless the models and a clip were pushed to the device:
 *
 *     adb push whisper-tiny whisper-base speech.wav /data/local/tmp/bench/
 *     adb shell run-as io.github.mehmetefeozdemir.lectio cp -r /data/local/tmp/bench files/
 *     adb shell am instrument -w -e class org.lectio.android.WhisperBenchmarkTest -e bench true \
 *         io.github.mehmetefeozdemir.lectio.test/androidx.test.runner.AndroidJUnitRunner
 *
 * An emulator runs on the computer's processor; a real phone is needed for
 * real numbers (the Transcript tab shows them after each run).
 */
@RunWith(AndroidJUnit4::class)
class WhisperBenchmarkTest {
    @Test
    fun transcriptionSpeed() {
        assumeTrue("Pass -e bench true to run the Whisper benchmark.", InstrumentationRegistry.getArguments().getString("bench") == "true")
        val folder = File(ApplicationProvider.getApplicationContext<Context>().filesDir, "bench")
        val wav = File(folder, "speech.wav").path
        val lines = mutableListOf<String>()
        for (name in listOf("tiny", "base")) {
            val models = File(folder, "whisper-$name")
            if (!models.exists()) continue
            val loadStart = System.nanoTime()
            val engine = SpeechEngine.load(models.path, name)
            val loadSeconds = (System.nanoTime() - loadStart) / 1e9
            val result = engine.transcribeWav(wav, "en")
            val line = String.format(
                java.util.Locale.US,
                "whisper-%s: load %.2f s, %.1f s of audio in %.2f s (%.1fx real time) | %s",
                name, loadSeconds, result.audioSeconds, result.seconds, result.audioSeconds / result.seconds, result.text,
            )
            Log.i("LectioBench", line)
            lines += line
            assertTrue(result.text.isNotBlank())
        }
        assertTrue("no whisper-tiny or whisper-base folder in $folder", lines.isNotEmpty())
        // Kept for Gradle runs; a run started with "am instrument" has no test
        // storage service, and the lines above are already in logcat.
        runCatching { TestStorage().openOutputFile("bench.txt").use { it.write(lines.joinToString("\n").toByteArray()) } }
            .onFailure { Log.i("LectioBench", "results only in logcat: ${it.message}") }
    }
}
