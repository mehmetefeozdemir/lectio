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
import org.lectio.android.support.nowUnixMs
import uniffi.lectio_ffi.CardInfo
import uniffi.lectio_ffi.Grade
import uniffi.lectio_ffi.LectioLibrary
import uniffi.lectio_ffi.ModelInfo
import uniffi.lectio_ffi.QuizQuestionInfo
import uniffi.lectio_ffi.StudyEngine
import uniffi.lectio_ffi.gradeAnswer
import uniffi.lectio_ffi.studyModels

/**
 * Study tools for one lecture, with the desktop's rules from the core: a
 * quiz from Definition notes (no AI needed), flashcards scheduled with SM-2,
 * and the study model's outline and questions (each citing a moment).
 */
class StudyViewModel(
    private val sessionId: String,
    private val library: LectioLibrary,
    private val models: ModelProvider? = null,
    private val changed: () -> Unit = {},
    private val now: () -> ULong = ::nowUnixMs,
    private val work: CoroutineDispatcher = Dispatchers.Default,
) : ViewModel() {
    data class Quiz(
        val questions: List<QuizQuestionInfo>,
        val index: Int = 0,
        val correct: Int = 0,
        /** The verdict on the current question, once answered. */
        val lastAnswerRight: Boolean? = null,
    ) {
        val current: QuizQuestionInfo? get() = questions.getOrNull(index)
        val isOver: Boolean get() = index >= questions.size
    }

    data class Busy(val text: String, val progress: Double)

    data class State(
        val quiz: Quiz? = null,
        val cards: List<CardInfo> = emptyList(),
        val showsBack: Boolean = false,
        val busy: Busy? = null,
        val problem: String? = null,
        val message: String? = null,
    ) {
        val currentCard: CardInfo? get() = cards.firstOrNull()
    }

    private val _state = MutableStateFlow(State())
    val state: StateFlow<State> = _state.asStateFlow()
    private var job: JobProgress? = null

    // Quiz

    /** Fill-in-the-blank questions from this lecture's Definition notes. */
    fun startOfflineQuiz() {
        try {
            val questions = library.offlineQuiz(listOf(sessionId), QUIZ_LENGTH)
            if (questions.isEmpty()) {
                _state.update { it.copy(message = "Label a few notes as Definition (\"Term: meaning\") to get a quiz without AI.") }
            } else {
                _state.update { it.copy(quiz = Quiz(questions), message = null) }
            }
        } catch (e: Exception) {
            fail(e)
        }
    }

    /** Checks a typed or chosen answer (every word of the answer, ignoring case, accents and punctuation). */
    fun answer(given: String) {
        val quiz = _state.value.quiz ?: return
        val question = quiz.current ?: return
        if (quiz.lastAnswerRight != null) return
        val right = if (question.choices.isEmpty()) gradeAnswer(given, question.answer) else given == question.answer
        _state.update { it.copy(quiz = quiz.copy(lastAnswerRight = right, correct = quiz.correct + if (right) 1 else 0)) }
    }

    fun nextQuestion() {
        val quiz = _state.value.quiz ?: return
        val next = quiz.copy(index = quiz.index + 1, lastAnswerRight = null)
        _state.update { it.copy(quiz = next) }
        if (next.isOver) finishQuiz()
    }

    /** Saves the score with the lecture (as the desktop does). */
    fun finishQuiz() {
        val quiz = _state.value.quiz ?: return
        val asked = minOf(quiz.index, quiz.questions.size)
        if (asked > 0) {
            try {
                library.recordQuizAttempt(sessionId, asked.toUInt(), quiz.correct.toUInt())
                changed()
            } catch (e: Exception) {
                fail(e)
            }
        }
        _state.update { it.copy(quiz = null, message = "Score: ${quiz.correct} of $asked") }
    }

    // Flashcards

    fun makeCards() {
        try {
            val made = library.makeDefinitionCards(sessionId, now()).toInt()
            val text = if (made == 0) "No new Definition notes to make cards from." else "Made $made card${if (made == 1) "" else "s"}."
            _state.update { it.copy(message = text) }
            loadDueCards()
            changed()
        } catch (e: Exception) {
            fail(e)
        }
    }

    /** Cards due now, from this lecture. */
    fun loadDueCards() {
        try {
            val due = library.dueCards(now()).filter { it.sessionId == sessionId }
            _state.update { it.copy(cards = due, showsBack = false) }
        } catch (e: Exception) {
            fail(e)
        }
    }

    fun flipCard() = _state.update { it.copy(showsBack = !it.showsBack) }

    fun grade(grade: Grade) {
        val card = _state.value.currentCard ?: return
        try {
            library.gradeCard(card.id, grade, now())
            loadDueCards()
            changed()
        } catch (e: Exception) {
            fail(e)
        }
    }

    // The study model

    val studyModel: ModelInfo? get() = studyModels().firstOrNull { it.name == STUDY_MODEL }

    fun writeOutline() = runJob("Writing the outline") { engine, progress ->
        engine.writeOutline(library, sessionId, progress)
        changed()
    }

    fun writeQuiz() = runJob("Writing questions") { engine, progress ->
        val questions = engine.writeQuiz(library, sessionId, AI_QUIZ_LENGTH, progress)
        _state.update { it.copy(quiz = Quiz(questions)) }
    }

    fun cancelJob() {
        job?.cancel()
    }

    fun clearProblem() = _state.update { it.copy(problem = null) }

    private fun runJob(title: String, body: suspend (StudyEngine, JobProgress) -> Unit) {
        val provider = models ?: return
        val model = studyModel ?: return
        if (_state.value.busy != null) return
        viewModelScope.launch {
            try {
                if (!provider.isDownloaded(model)) {
                    _state.update { it.copy(busy = Busy("Downloading the study model", 0.0)) }
                    provider.download(model) { f -> _state.update { it.copy(busy = Busy("Downloading the study model", f)) } }
                }
                _state.update { it.copy(busy = Busy("Loading the study model", 0.0)) }
                val engine = provider.studyEngine(model)
                val progress = JobProgress { f -> _state.update { it.copy(busy = Busy(title, f)) } }
                job = progress
                _state.update { it.copy(busy = Busy(title, 0.0)) }
                withContext(work) { body(engine, progress) }
            } catch (e: Exception) {
                fail(e)
            } finally {
                job = null
                _state.update { it.copy(busy = null) }
            }
        }
    }

    private fun fail(e: Exception) = _state.update { it.copy(problem = errorText(e)) }

    companion object {
        const val QUIZ_LENGTH = 10u
        const val AI_QUIZ_LENGTH = 6u
        /** The smaller study model: fits in a phone's memory. */
        const val STUDY_MODEL = "small"
    }
}
