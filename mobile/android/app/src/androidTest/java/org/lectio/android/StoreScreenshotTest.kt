package org.lectio.android

import android.graphics.Bitmap
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasStateDescription
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToNode
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.services.storage.TestStorage
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The Google Play screenshots (ADR-039): the sample lecture on Home, its
 * notes while playing from a note, the slides with ink, the transcript and a
 * quiz. Run by tools/store/screenshots.sh, which sets the screen size for
 * each form factor and Android's demo-mode status bar first:
 *
 *     -e storeScreenshots true -e formFactor phone|tablet7|tablet10
 *
 * Skipped in ordinary test runs.
 */
@OptIn(ExperimentalTestApi::class)
@RunWith(AndroidJUnit4::class)
class StoreScreenshotTest {
    @get:Rule val compose = createEmptyComposeRule()

    private val form: String = InstrumentationRegistry.getArguments().getString("formFactor") ?: "phone"

    private fun shot(name: String) {
        compose.waitForIdle()
        Thread.sleep(SETTLE_MS)
        val bitmap = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot() ?: return
        TestStorage().openOutputFile("store/$form-$name.png").use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }

    @Test
    fun storeScreenshots() {
        assumeTrue("Run by tools/store/screenshots.sh", InstrumentationRegistry.getArguments().getString("storeScreenshots") == "true")
        val app = ApplicationProvider.getApplicationContext<LectioApplication>()
        app.startForTests()
        app.graph!!.library.addSampleLibrary()

        ActivityScenario.launch(MainActivity::class.java).use {
            compose.waitUntilAtLeastOneExists(hasText("Graph traversal"), WAIT_MS)
            if (form == "phone") shot("01-home")

            // The lecture, playing from the note at 17:05, then paused there.
            compose.onNodeWithText("Graph traversal").performClick()
            compose.waitUntilExactlyOneExists(hasTestTag("audio.play"), WAIT_MS)
            // On a phone the note is below the fold of the lazy list, so it
            // isn't composed until the list scrolls to it.
            val note = hasContentDescription("Play from 17 minutes, 5 seconds")
            compose.onNodeWithTag("notes.list").performScrollToNode(note)
            compose.onNode(note).performClick()
            compose.waitUntilExactlyOneExists(hasContentDescription("Pause"), WAIT_MS)
            compose.onNodeWithTag("audio.play").performClick()
            shot(if (form == "phone") "02-notes" else "01-lecture")

            compose.onNodeWithTag("tab.slides").performClick()
            compose.waitUntilExactlyOneExists(hasContentDescription("Next page"), WAIT_MS)
            compose.onNodeWithContentDescription("Next page").performClick()
            // The page renders and its ink loads off the main thread; the
            // slide says how many strokes it shows once both are in.
            compose.waitUntilExactlyOneExists(hasContentDescription("Slide 2") and hasStateDescription("2 ink strokes"), WAIT_MS)
            shot("03-slides")

            compose.onNodeWithTag("tab.transcript").performClick()
            compose.waitUntilAtLeastOneExists(hasText("So let's look at breadth-first search."), WAIT_MS)
            shot("04-transcript")

            compose.onNodeWithTag("tab.study").performClick()
            compose.onNodeWithText("Quiz from my Definition notes").performClick()
            compose.waitUntilAtLeastOneExists(hasText("Question 1 of 3"), WAIT_MS)
            shot("05-quiz")
        }
    }

    private companion object {
        const val WAIT_MS = 15_000L
        /** Lets animations (ripples, the tab indicator) finish. */
        const val SETTLE_MS = 800L
    }
}
