package org.lectio.android

import java.nio.file.Files
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestDispatcher
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import org.junit.rules.TestWatcher
import org.junit.runner.Description
import uniffi.lectio_ffi.LectioLibrary

/**
 * A library in a fresh temporary folder. These tests run on the computer's
 * JVM and call the real Rust core (built for this computer by
 * tools/mobile/build-android.sh) through JNA.
 */
fun makeLibrary(): LectioLibrary = LectioLibrary.open(Files.createTempDirectory("lectio-test").toString())

/** A lecture in a fresh library: (library, session id). */
fun makeSession(title: String = "Cells"): Pair<LectioLibrary, String> {
    val library = makeLibrary()
    val course = library.addCourse("Biology")
    return library to library.addSession(course, title)
}

/** `viewModelScope` runs on Dispatchers.Main; here that's a test dispatcher. */
@OptIn(ExperimentalCoroutinesApi::class)
class MainDispatcherRule(val dispatcher: TestDispatcher = UnconfinedTestDispatcher()) : TestWatcher() {
    override fun starting(description: Description) = Dispatchers.setMain(dispatcher)
    override fun finished(description: Description) = Dispatchers.resetMain()
}
