package org.lectio.android

import android.Manifest
import android.accessibilityservice.AccessibilityService
import android.graphics.Bitmap
import android.os.Build
import android.os.SystemClock
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextClearance
import androidx.compose.ui.test.performTextInput
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.GrantPermissionRule
import androidx.test.services.storage.TestStorage
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import uniffi.lectio_ffi.HomeFilter
import uniffi.lectio_ffi.SessionSort
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/** Saves the whole screen (system sheets included) for the CI artifacts. */
fun screenshot(name: String) {
    val bitmap = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot() ?: return
    TestStorage().openOutputFile("screenshots/$name.png").use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
}

/**
 * The core flow, as a student would do it: create a course and a lecture,
 * record about 3 seconds while writing a note, play from the note, export a
 * PDF to the share sheet. The app runs on an empty library with a test tone
 * instead of the microphone (CI emulators have no audio input).
 */
@OptIn(ExperimentalTestApi::class)
@RunWith(AndroidJUnit4::class)
class CoreFlowTest {
    @get:Rule val compose = createEmptyComposeRule()

    @get:Rule val permissions: GrantPermissionRule = GrantPermissionRule.grant(
        *listOfNotNull(Manifest.permission.RECORD_AUDIO, if (Build.VERSION.SDK_INT >= 33) Manifest.permission.POST_NOTIFICATIONS else null).toTypedArray(),
    )

    @Test
    fun createCourseRecordNotePlayAndExport() {
        val app = ApplicationProvider.getApplicationContext<LectioApplication>()
        app.startForTests()
        ActivityScenario.launch(MainActivity::class.java).use {
            compose.waitUntilExactlyOneExists(hasTestTag("home.newCourse"), WAIT_MS)
            compose.waitForIdle()
            screenshot("1-empty-library")

            // A course and a lecture in it.
            compose.onNodeWithTag("home.newCourse").performClick()
            compose.onNodeWithTag("home.courseName").performTextInput("Biology")
            compose.onNodeWithTag("dialog.create").performClick()
            compose.waitUntilExactlyOneExists(hasTestTag("home.newLecture"), WAIT_MS)
            compose.onNodeWithTag("home.newLecture").performClick()
            compose.onNodeWithTag("newLecture.title").performTextClearance()
            compose.onNodeWithTag("newLecture.title").performTextInput("Cells")
            compose.onNodeWithTag("newLecture.create").performClick()

            // Record, writing a note while it runs (stamped when typing starts).
            compose.waitUntilExactlyOneExists(hasTestTag("audio.record"), WAIT_MS)
            compose.onNodeWithTag("audio.record").performClick()
            compose.waitUntilExactlyOneExists(hasTestTag("audio.stop"), WAIT_MS)
            val started = SystemClock.elapsedRealtime()
            compose.onNodeWithTag("composer.text").performTextInput("Mitochondria make ATP")
            compose.onNodeWithTag("composer.add").performClick()
            compose.waitUntilExactlyOneExists(hasText("Mitochondria make ATP"), WAIT_MS)
            val left = RECORDING_MS - (SystemClock.elapsedRealtime() - started)
            if (left > 0) Thread.sleep(left)
            compose.waitForIdle()
            screenshot("2-recording-with-a-note")
            compose.onNodeWithTag("audio.stop").performClick()

            // Play from the note.
            compose.waitUntilExactlyOneExists(hasTestTag("audio.play"), WAIT_MS)
            val recorded = app.graph!!.library.sessions(HomeFilter.All, SessionSort.NEWEST).first().durationMs ?: 0uL
            assertTrue("recorded $recorded ms, wanted at least 3 s", recorded >= 3_000uL)
            compose.onNodeWithTag("note.play").performClick()
            compose.waitUntilExactlyOneExists(hasContentDescription("Pause"), WAIT_MS)
            compose.waitForIdle()
            screenshot("3-playing-from-the-note")

            // Export a PDF through the share sheet. The system's sheet covers
            // the app, so the test checks the file the sheet was given.
            val exported = File(app.graph!!.exportDirectory, "Cells.pdf")
            compose.onNodeWithTag("session.export").performClick()
            compose.onNodeWithTag("export.pdf").performClick()
            compose.waitForIdle()
            val automation = InstrumentationRegistry.getInstrumentation().uiAutomation
            val deadline = SystemClock.elapsedRealtime() + EXPORT_WAIT_MS
            fun sheetShown() = automation.rootInActiveWindow?.packageName?.toString().let { it != null && it != app.packageName }
            while (!(exported.exists() && sheetShown()) && SystemClock.elapsedRealtime() < deadline) Thread.sleep(POLL_MS)
            assertEquals("%PDF", String(exported.readBytes().copyOf(4)))
            assertTrue("the share sheet opened", sheetShown())
            Thread.sleep(SHEET_SETTLE_MS)
            screenshot("4-share-sheet")
            InstrumentationRegistry.getInstrumentation().uiAutomation.performGlobalAction(AccessibilityService.GLOBAL_ACTION_BACK)
        }
    }

    private companion object {
        const val WAIT_MS = 10_000L
        const val EXPORT_WAIT_MS = 30_000L
        const val RECORDING_MS = 3_500L
        const val SHEET_SETTLE_MS = 1_500L
        const val POLL_MS = 100L
    }
}
