package org.lectio.android.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Card
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import org.lectio.android.models.SessionViewModel
import org.lectio.android.models.StudyViewModel
import org.lectio.android.support.TimeText
import uniffi.lectio_ffi.Grade

/** Quiz, flashcards and outline for the lecture. */
@Composable
fun StudyTab(session: SessionViewModel, study: StudyViewModel, playFrom: (ULong) -> Unit) {
    val state by study.state.collectAsStateWithLifecycle()
    val sessionState by session.state.collectAsStateWithLifecycle()
    LaunchedEffect(Unit) { study.loadDueCards() }

    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        state.busy?.let { busy ->
            Text(busy.text, modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite })
            LinearProgressIndicator(progress = { busy.progress.toFloat() }, modifier = Modifier.fillMaxWidth())
            TextButton(onClick = study::cancelJob) { Text("Stop") }
        }
        state.message?.let { Text(it, modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite }) }

        Heading("Quiz")
        QuizSection(state, study, canPlay = sessionState.recordingPath != null, playFrom = playFrom, lastScore = sessionState.detail?.quizScores?.lastOrNull()?.let { "${it.correct} of ${it.asked}" })
        HorizontalDivider()

        Heading("Flashcards")
        val card = state.currentCard
        if (card == null) {
            Text("No cards due from this lecture.", color = MaterialTheme.colorScheme.onSurfaceVariant)
        } else {
            Card(Modifier.fillMaxWidth().clickable(onClickLabel = if (state.showsBack) "Hide the answer" else "Show the answer") { study.flipCard() }) {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(card.front, style = MaterialTheme.typography.titleMedium)
                    if (state.showsBack) Text(card.back) else Text("Tap to show the answer", style = MaterialTheme.typography.bodySmall)
                }
            }
            if (state.showsBack) {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    listOf(Grade.AGAIN to "Again", Grade.HARD to "Hard", Grade.GOOD to "Good", Grade.EASY to "Easy").forEach { (grade, label) ->
                        OutlinedButton(onClick = { study.grade(grade) }, modifier = Modifier.weight(1f)) { Text(label, maxLines = 1) }
                    }
                }
            }
            Text("${state.cards.size} due", style = MaterialTheme.typography.bodySmall)
        }
        OutlinedButton(onClick = study::makeCards) { Text("Make cards from Definition notes") }
        HorizontalDivider()

        Heading("Outline")
        val outline = sessionState.detail?.outline.orEmpty()
        if (outline.isEmpty()) {
            Text("The study model writes an outline of the lecture, each point linked to its moment.", color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        outline.forEach { item ->
            val offset = item.offsetMs
            Row(
                Modifier.fillMaxWidth()
                    .clickable(enabled = offset != null && sessionState.recordingPath != null) { offset?.let(playFrom) }
                    .padding(start = (maxOf(item.level.toInt() - 1, 0) * 16).dp, top = 4.dp, bottom = 4.dp),
            ) {
                Text(item.title, style = if (item.level <= 1u) MaterialTheme.typography.titleSmall else MaterialTheme.typography.bodyMedium, modifier = Modifier.weight(1f))
                offset?.let { Text(TimeText.clock(it), style = MaterialTheme.typography.labelSmall) }
            }
        }
        OutlinedButton(onClick = study::writeOutline, enabled = state.busy == null) { Text(if (outline.isEmpty()) "Write an outline" else "Write it again") }
    }
    ProblemDialog(state.problem, study::clearProblem)
}

@Composable
private fun Heading(text: String) = Text(text, style = MaterialTheme.typography.titleLarge, modifier = Modifier.semantics { heading() })

@Composable
private fun QuizSection(state: StudyViewModel.State, study: StudyViewModel, canPlay: Boolean, playFrom: (ULong) -> Unit, lastScore: String?) {
    val quiz = state.quiz
    val question = quiz?.current
    if (quiz == null || question == null) {
        OutlinedButton(onClick = study::startOfflineQuiz) { Text("Quiz from my Definition notes") }
        OutlinedButton(onClick = study::writeQuiz, enabled = state.busy == null) { Text("Write questions with the study model") }
        lastScore?.let { Text("Last score: $it", color = MaterialTheme.colorScheme.onSurfaceVariant) }
        return
    }
    var typed by remember(quiz.index) { mutableStateOf("") }
    Text("Question ${quiz.index + 1} of ${quiz.questions.size}", style = MaterialTheme.typography.bodySmall)
    Text(question.prompt, style = MaterialTheme.typography.titleMedium)
    if (question.choices.isEmpty()) {
        OutlinedTextField(typed, { typed = it }, label = { Text("Your answer") }, singleLine = true, modifier = Modifier.fillMaxWidth())
        OutlinedButton(onClick = { study.answer(typed) }, enabled = quiz.lastAnswerRight == null) { Text("Check") }
    } else {
        question.choices.forEach { choice ->
            OutlinedButton(onClick = { study.answer(choice) }, enabled = quiz.lastAnswerRight == null, modifier = Modifier.fillMaxWidth()) { Text(choice) }
        }
    }
    quiz.lastAnswerRight?.let { right ->
        Text(
            if (right) "Right" else "The answer is: ${question.answer}",
            color = if (right) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.error,
            modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite },
        )
        if (question.explanation.isNotBlank()) Text(question.explanation)
        val offset = question.offsetMs
        if (offset != null && canPlay) TextButton(onClick = { playFrom(offset) }) { Text("Play that moment (${TimeText.clock(offset)})") }
        OutlinedButton(onClick = study::nextQuestion) { Text("Next") }
    }
}
