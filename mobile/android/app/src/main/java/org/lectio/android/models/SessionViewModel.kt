package org.lectio.android.models

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import java.io.File
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.lectio.android.support.TimeText
import org.lectio.android.support.errorText
import org.lectio.android.support.fileSize
import uniffi.lectio_ffi.ExportFormat
import uniffi.lectio_ffi.LectioLibrary
import uniffi.lectio_ffi.NoteDraft
import uniffi.lectio_ffi.NoteInfo
import uniffi.lectio_ffi.NoteKind
import uniffi.lectio_ffi.NoteLabel
import uniffi.lectio_ffi.SessionDetail
import uniffi.lectio_ffi.SlideRef

/**
 * One lecture: its notes, slides, transcript and study material, and every
 * change to it. Changes go through the core and are saved at once in the
 * desktop's format.
 */
class SessionViewModel(
    val sessionId: String,
    val library: LectioLibrary,
    private val changed: () -> Unit = {},
    /** Where exports are written for the share sheet (the cache's exports/). */
    private val exportDir: File,
    private val io: CoroutineDispatcher = Dispatchers.IO,
) : ViewModel() {
    data class State(
        val detail: SessionDetail? = null,
        val draft: String = "",
        val draftLabels: List<NoteLabel> = emptyList(),
        /** The slide on screen in the Slides tab, for "link note to this slide". */
        val currentSlide: SlideRef? = null,
        val problem: String? = null,
        /** The last thing that happened, shown briefly and announced by TalkBack. */
        val status: String? = null,
        val isExporting: Boolean = false,
    ) {
        val notes: List<NoteInfo> get() = detail?.notes.orEmpty()
        val title: String get() = detail?.info?.title.orEmpty()
        val recordingPath: String? get() = detail?.recordingPath

        fun notesOn(slide: SlideRef): List<NoteInfo> =
            notes.filter { it.slide?.deckId == slide.deckId && it.slide?.page == slide.page && it.kind != NoteKind.SLIDE }
    }

    private val _state = MutableStateFlow(State())
    val state: StateFlow<State> = _state.asStateFlow()

    /** Remembers when typing started (the core's rule, as on the desktop and iOS). */
    private val draftClock = NoteDraft()

    fun reload() {
        try {
            val detail = library.session(sessionId)
            _state.update { it.copy(detail = detail) }
        } catch (e: Exception) {
            fail(e)
        }
    }

    /** Every edit of the draft, with the recording's moment [nowMs]. */
    fun editDraft(text: String, nowMs: ULong) {
        _state.update { it.copy(draft = text) }
        draftClock.setText(text, nowMs)
    }

    fun toggleDraftLabel(label: NoteLabel) = _state.update {
        it.copy(draftLabels = if (label in it.draftLabels) it.draftLabels - label else it.draftLabels + label)
    }

    /** Adds the draft as a note stamped when typing started (or at [nowMs]). */
    fun addNote(nowMs: ULong): Boolean {
        val current = _state.value
        val text = current.draft.trim()
        if (text.isEmpty()) return false
        val offset = draftClock.anchorMs(nowMs)
        if (!perform { library.addNote(sessionId, offset, text, current.draftLabels) }) return false
        _state.update { it.copy(draft = "", draftLabels = emptyList()) }
        draftClock.setText("", nowMs)
        tell("Note added at ${TimeText.clock(offset)}")
        return true
    }

    fun addFormula(latex: String, atMs: ULong): Boolean {
        val source = latex.trim()
        if (source.isEmpty()) return false
        return perform { library.addFormulaNote(sessionId, atMs, source, emptyList()) }
            .also { if (it) tell("Formula added at ${TimeText.clock(atMs)}") }
    }

    /** [jpegOrPng]: the picture as JPEG or PNG (the screen converts other formats). */
    fun addPhoto(jpegOrPng: ByteArray, caption: String, atMs: ULong): Boolean =
        perform { library.addPhotoNote(sessionId, atMs, jpegOrPng, caption) }
            .also { if (it) tell("Photo added at ${TimeText.clock(atMs)}") }

    fun toggle(label: NoteLabel, note: NoteInfo) {
        perform { library.toggleLabel(sessionId, note.id, label) }
    }

    fun edit(note: NoteInfo, text: String) {
        perform { library.editNoteText(sessionId, note.id, text) }
    }

    fun delete(note: NoteInfo) {
        if (perform { library.deleteNote(sessionId, note.id) }) tell("Note deleted")
    }

    fun rename(title: String) {
        perform { library.renameSession(sessionId, title.trim()) }
    }

    /** Links a note to [slide], or unlinks it (null). */
    fun link(note: NoteInfo, slide: SlideRef?) {
        perform { library.linkNoteToSlide(sessionId, note.id, slide?.deckId, slide?.page ?: 0u) }
    }

    fun setCurrentSlide(slide: SlideRef?) = _state.update { it.copy(currentSlide = slide) }

    /** Imports a PDF's bytes; returns the new deck's id. */
    fun importSlides(fileName: String, bytes: ByteArray): String? = try {
        library.importSlides(sessionId, fileName, bytes).also {
            reload()
            changed()
            tell("Slides imported")
        }
    } catch (e: Exception) {
        fail(e)
        null
    }

    /** Puts a slide into the note feed at [atMs]. */
    fun postSlide(slide: SlideRef, atMs: ULong) {
        if (perform { library.postSlide(sessionId, atMs, slide.deckId, slide.page) }) tell("Slide ${slide.page} added to the notes")
    }

    /** Writes the lecture as a document for the share sheet, off the main thread. */
    suspend fun export(format: ExportFormat, includeTranscript: Boolean): File? {
        _state.update { it.copy(isExporting = true) }
        return try {
            val file = withContext(io) {
                val name = library.exportFileName(sessionId, format)
                val bytes = library.export(sessionId, format, includeTranscript)
                exportDir.mkdirs()
                File(exportDir, name).apply { writeBytes(bytes) }
            }
            tell("Exported ${file.name}, ${fileSize(file.length())}")
            file
        } catch (e: Exception) {
            fail(e)
            null
        } finally {
            _state.update { it.copy(isExporting = false) }
        }
    }

    fun clearProblem() = _state.update { it.copy(problem = null) }

    private fun tell(text: String) {
        _state.update { it.copy(status = text) }
        viewModelScope.launch {
            delay(STATUS_MS)
            _state.update { if (it.status == text) it.copy(status = null) else it }
        }
    }

    private fun fail(e: Exception) = _state.update { it.copy(problem = errorText(e)) }

    private fun perform(body: () -> Unit): Boolean = try {
        body()
        reload()
        changed()
        true
    } catch (e: Exception) {
        fail(e)
        false
    }

    override fun onCleared() {
        draftClock.close()
    }

    companion object {
        /** How long a status line ("Note added at 0:12") stays on screen. */
        const val STATUS_MS = 4_000L
    }
}
