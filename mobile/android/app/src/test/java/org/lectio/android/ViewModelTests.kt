package org.lectio.android

import java.nio.file.Files
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.lectio.android.models.HomeViewModel
import org.lectio.android.models.SessionViewModel
import org.lectio.android.models.StudyViewModel
import uniffi.lectio_ffi.ExportFormat
import uniffi.lectio_ffi.Grade
import uniffi.lectio_ffi.HomeFilter
import uniffi.lectio_ffi.NoteKind
import uniffi.lectio_ffi.NoteLabel
import uniffi.lectio_ffi.checkFormula

class HomeViewModelTest {
    @get:Rule val main = MainDispatcherRule()

    @Test
    fun creatingACourseAndALectureShowsThemOnHome() {
        // Arrange
        val home = HomeViewModel(makeLibrary())
        // Act
        val course = home.addCourse("  Biology  ")!!
        val session = home.addSession(course, "Cells")!!
        // Assert
        val state = home.state.value
        assertEquals(listOf("Biology"), state.courses.map { it.name })
        assertEquals(listOf(session), state.sessions.map { it.id })
        assertEquals("Biology", state.sessions.first().courseName)
        assertNull(state.problem)
    }

    @Test
    fun anEmptyCourseNameIsRefusedWithAMessage() {
        val home = HomeViewModel(makeLibrary())
        assertNull(home.addCourse("   "))
        assertNotNull(home.state.value.problem)
    }

    @Test
    fun filteringByCourseAndCollection() {
        val home = HomeViewModel(makeLibrary())
        val biology = home.addCourse("Biology")!!
        val history = home.addCourse("History")!!
        val cells = home.addSession(biology, "Cells")!!
        home.addSession(history, "Rome")
        val exam = home.addCollection("Exam")!!
        home.addToCollection(cells, exam)

        home.setFilter(HomeFilter.Course(history))
        assertEquals(listOf("Rome"), home.state.value.sessions.map { it.title })
        assertEquals("History", home.state.value.filterTitle)

        home.setFilter(HomeFilter.Collection(exam))
        assertEquals(listOf("Cells"), home.state.value.sessions.map { it.title })
        assertEquals(listOf("Exam"), home.state.value.collectionsContaining(cells).map { it.name })
    }

    @Test
    fun aDeletedCollectionFilterFallsBackToAllLectures() {
        val home = HomeViewModel(makeLibrary())
        home.addSession(home.addCourse("Biology")!!, "Cells")
        val exam = home.addCollection("Exam")!!
        home.setFilter(HomeFilter.Collection(exam))
        home.deleteCollection(exam)
        assertEquals(HomeFilter.All, home.state.value.filter)
        assertEquals(1, home.state.value.sessions.size)
    }

    @Test
    fun searchFindsANoteAndTrashHidesTheLecture() {
        val library = makeLibrary()
        val home = HomeViewModel(library)
        val session = home.addSession(home.addCourse("Biology")!!, "Cells")!!
        library.addNote(session, 4_000uL, "Mitochondria make ATP", emptyList())

        home.setQuery("mitochondria")
        assertEquals(session, home.state.value.hits.first().sessionId)
        assertEquals(4_000uL, home.state.value.hits.first().offsetMs)

        home.setQuery("")
        home.trash(session)
        assertTrue(home.state.value.sessions.isEmpty())
    }

    @Test
    fun changesTellTheApp() {
        var changes = 0
        val home = HomeViewModel(makeLibrary()) { changes += 1 }
        home.addCourse("Biology")
        assertEquals(1, changes)
    }
}

@OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class)
class SessionViewModelTest {
    @get:Rule val main = MainDispatcherRule()

    private fun makeModel(): SessionViewModel {
        val (library, session) = makeSession()
        return SessionViewModel(session, library, exportDir = Files.createTempDirectory("exports").toFile(), io = UnconfinedTestDispatcher())
            .also { it.reload() }
    }

    @Test
    fun aNoteKeepsItsMomentAndLabelsAndClearsTheDraft() {
        val model = makeModel()
        model.editDraft("  ATP is the energy currency  ", 12_345uL)
        model.toggleDraftLabel(NoteLabel.IMPORTANT)
        model.toggleDraftLabel(NoteLabel.REVIEW)
        model.toggleDraftLabel(NoteLabel.REVIEW)

        assertTrue(model.addNote(20_000uL))

        val state = model.state.value
        val note = state.notes.first()
        assertEquals("ATP is the energy currency", note.body)
        assertEquals("stamped when typing started", 12_345uL, note.offsetMs)
        assertEquals(listOf(NoteLabel.IMPORTANT), note.labels)
        assertEquals("", state.draft)
        assertEquals(emptyList<NoteLabel>(), state.draftLabels)
        assertEquals("Note added at 0:12", state.status)
    }

    @Test
    fun anEmptyDraftAddsNothing() {
        val model = makeModel()
        model.editDraft("   ", 0uL)
        assertFalse(model.addNote(0uL))
        assertTrue(model.state.value.notes.isEmpty())
    }

    @Test
    fun labelsToggleEditAndDelete() {
        val model = makeModel()
        model.editDraft("Osmosis", 1_000uL)
        model.addNote(1_000uL)

        model.toggle(NoteLabel.DEFINITION, model.state.value.notes.first())
        assertEquals(listOf(NoteLabel.DEFINITION), model.state.value.notes.first().labels)

        model.edit(model.state.value.notes.first(), "Osmosis: water crossing a membrane")
        assertEquals("Osmosis: water crossing a membrane", model.state.value.notes.first().body)

        model.delete(model.state.value.notes.first())
        assertTrue(model.state.value.notes.isEmpty())
    }

    @Test
    fun formulaNotesAreCheckedByTheCore() {
        val model = makeModel()
        assertTrue(model.addFormula("E = mc^2", 2_000uL))
        assertEquals(NoteKind.FORMULA, model.state.value.notes.first().kind)
        assertNull(checkFormula("\\frac{a}{b}"))
        assertNotNull(checkFormula("\\frac{a}{"))
    }

    @Test
    fun exportWritesAFileForTheShareSheet() = runTest {
        val model = makeModel()
        model.editDraft("Mitochondria make ATP", 3_000uL)
        model.addNote(3_000uL)

        val markdown = model.export(ExportFormat.MARKDOWN, includeTranscript = false)!!
        val pdf = model.export(ExportFormat.PDF, includeTranscript = false)!!

        assertTrue(markdown.readText().contains("Mitochondria make ATP"))
        assertEquals("Cells.md", markdown.name)
        assertEquals("%PDF", String(pdf.readBytes().copyOf(4)))
        assertTrue(model.state.value.status!!.startsWith("Exported Cells.pdf"))
        assertFalse(model.state.value.isExporting)
    }
}

class StudyViewModelTest {
    @get:Rule val main = MainDispatcherRule()

    @Test
    fun theOfflineQuizComesFromDefinitionNotesAndSavesTheScore() {
        val (library, session) = makeSession()
        library.addNote(session, 1_000uL, "Osmosis: water moving across a membrane", listOf(NoteLabel.DEFINITION))
        val study = StudyViewModel(session, library)

        study.startOfflineQuiz()
        val question = study.state.value.quiz!!.current!!
        study.answer(question.answer.uppercase())
        assertEquals(true, study.state.value.quiz!!.lastAnswerRight)
        study.nextQuestion()

        assertNull(study.state.value.quiz)
        assertEquals("Score: 1 of 1", study.state.value.message)
        assertEquals(1u, library.session(session).quizScores.last().correct)
    }

    @Test
    fun withoutDefinitionsTheQuizExplainsWhatToDo() {
        val (library, session) = makeSession()
        val study = StudyViewModel(session, library)
        study.startOfflineQuiz()
        assertNull(study.state.value.quiz)
        assertTrue(study.state.value.message!!.contains("Definition"))
    }

    @Test
    fun flashcardsAreMadeAndScheduledAfterGrading() {
        val (library, session) = makeSession()
        library.addNote(session, 1_000uL, "ATP: the cell's energy currency", listOf(NoteLabel.DEFINITION))
        val study = StudyViewModel(session, library, now = { 1_800_000_000_000uL })

        study.makeCards()
        assertEquals(1, study.state.value.cards.size)
        assertEquals("ATP", study.state.value.currentCard!!.front)

        study.grade(Grade.GOOD)
        assertTrue("a card known well isn't due again at once", study.state.value.cards.isEmpty())
    }
}
