package org.lectio.android

import android.app.Application
import android.content.Context
import android.util.Log
import java.io.File
import kotlinx.coroutines.MainScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import org.lectio.android.audio.AndroidAudioHost
import org.lectio.android.audio.AudioOutput
import org.lectio.android.audio.AudioSource
import org.lectio.android.audio.ClockOutput
import org.lectio.android.audio.MicrophoneSource
import org.lectio.android.audio.PlayerController
import org.lectio.android.audio.RecorderController
import org.lectio.android.audio.ToneSource
import org.lectio.android.audio.TrackOutput
import org.lectio.android.models.ModelProvider
import org.lectio.android.models.ModelStore
import org.lectio.android.support.errorText
import uniffi.lectio_ffi.LectioLibrary

/** The library and the audio, shared by every screen and the recording service. */
class AppGraph(
    val library: LectioLibrary,
    val recorder: RecorderController,
    val player: PlayerController,
    val models: ModelProvider,
    /** Where exports go before the share sheet (a FileProvider path). */
    val exportDirectory: File,
    /** Where camera photos go before they're imported (a FileProvider path). */
    val cameraDirectory: File,
    /** False with the test tone: nothing to ask permission for. */
    val needsMicrophonePermission: Boolean,
) {
    private val _revision = MutableStateFlow(0)

    /** Bumped after every change, so screens that show library data reload. */
    val revision: StateFlow<Int> = _revision.asStateFlow()

    fun changed() = _revision.update { it + 1 }
}

/**
 * Opens the library once. The notes live in the app's files folder, in
 * `Lectio/` with the desktop's layout (library.json, recordings/, blobs/,
 * models/). Instrumented tests call [startForTests]: an empty library and the
 * test tone instead of the microphone (emulators in CI have no audio).
 */
class LectioApplication : Application() {
    var graph: AppGraph? = null
        private set
    var startupProblem: String? = null
        private set

    override fun onCreate() {
        super.onCreate()
        start(File(filesDir, "Lectio"), testAudio = false)
    }

    fun startForTests() {
        val folder = File(filesDir, "LectioTest")
        folder.deleteRecursively()
        start(folder, testAudio = true)
    }

    private fun start(dataDirectory: File, testAudio: Boolean) {
        try {
            dataDirectory.mkdirs()
            val library = LectioLibrary.open(dataDirectory.path)
            graph = buildGraph(this, library, dataDirectory, testAudio)
            startupProblem = null
        } catch (e: Exception) {
            Log.e("Lectio", "Couldn't open the library", e)
            graph = null
            startupProblem = errorText(e)
        }
    }

    private fun buildGraph(context: Context, library: LectioLibrary, dataDirectory: File, testAudio: Boolean): AppGraph {
        val scope = MainScope()
        val host = AndroidAudioHost(context)
        val makeSource: () -> AudioSource = if (testAudio) ({ ToneSource() }) else ({ MicrophoneSource(context) })
        val output: AudioOutput = if (testAudio) ClockOutput() else TrackOutput()
        val recorder = RecorderController(library, makeSource, host, scope)
        val player = PlayerController(output, host, scope)
        host.recorder = recorder
        host.player = player
        val graph = AppGraph(
            library = library,
            recorder = recorder,
            player = player,
            models = ModelStore(context, dataDirectory),
            exportDirectory = File(cacheDir, "exports"),
            cameraDirectory = File(cacheDir, "camera"),
            needsMicrophonePermission = !testAudio,
        )
        recorder.onFinished = { graph.changed() }
        return graph
    }
}
