package org.lectio.android.models

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.lectio.android.support.errorText
import org.lectio.android.support.transcriptionDoneText
import uniffi.lectio_ffi.LectioLibrary
import uniffi.lectio_ffi.ModelInfo
import uniffi.lectio_ffi.speechModels

/**
 * Transcribes a lecture on the device with Whisper (the desktop's engine).
 * The first run downloads the model. Segments are saved as each 30-second
 * window finishes, so the transcript fills in while it runs and a stopped
 * job keeps what it has. The finished line shows the speed, so a real
 * device can be measured from the screen.
 */
class TranscriptionViewModel(
    private val sessionId: String,
    private val library: LectioLibrary,
    private val models: ModelProvider,
    /** Runs on the main thread each time a window is saved. */
    private val stored: () -> Unit = {},
    private val work: CoroutineDispatcher = Dispatchers.Default,
) : ViewModel() {
    sealed interface Phase {
        data object Idle : Phase
        data class Downloading(val fraction: Double) : Phase
        data object Loading : Phase
        data class Transcribing(val fraction: Double) : Phase
        data class Finished(val text: String) : Phase
        data class Failed(val message: String) : Phase
    }

    data class State(val phase: Phase = Phase.Idle, val modelName: String = "base") {
        val isRunning: Boolean get() = phase is Phase.Downloading || phase is Phase.Loading || phase is Phase.Transcribing
    }

    val available: List<ModelInfo> = speechModels()
    private val _state = MutableStateFlow(State())
    val state: StateFlow<State> = _state.asStateFlow()
    private var job: JobProgress? = null

    fun chooseModel(name: String) {
        if (!_state.value.isRunning) _state.update { it.copy(modelName = name) }
    }

    fun transcribe() {
        val model = available.firstOrNull { it.name == _state.value.modelName } ?: return
        if (_state.value.isRunning) return
        viewModelScope.launch {
            try {
                if (!models.isDownloaded(model)) {
                    setPhase(Phase.Downloading(0.0))
                    models.download(model) { fraction -> setPhase(Phase.Downloading(fraction)) }
                }
                setPhase(Phase.Loading)
                val engine = models.speechEngine(model)
                setPhase(Phase.Transcribing(0.0))
                val progress = JobProgress { fraction ->
                    setPhase(Phase.Transcribing(fraction))
                    viewModelScope.launch { stored() }
                }
                job = progress
                val started = System.nanoTime()
                val segments = withContext(work) { engine.transcribeSession(library, sessionId, null, progress) }
                val seconds = (System.nanoTime() - started) / 1e9
                val audioMs = library.session(sessionId).info.durationMs
                setPhase(Phase.Finished(transcriptionDoneText(segments, seconds, audioMs)))
                stored()
            } catch (e: Exception) {
                setPhase(Phase.Failed(errorText(e)))
            } finally {
                job = null
            }
        }
    }

    fun cancel() {
        job?.cancel()
    }

    private fun setPhase(phase: Phase) = _state.update { it.copy(phase = phase) }
}
