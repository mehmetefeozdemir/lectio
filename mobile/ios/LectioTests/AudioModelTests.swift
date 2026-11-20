import XCTest
@testable import Lectio

/// Pushes samples when the test says so, like a microphone on demand.
final class FakeSource: AudioSource {
    private(set) var resumed = 0
    private var sink: SampleSink?

    func start(_ sink: @escaping SampleSink) throws { self.sink = sink }
    func resume() throws { resumed += 1 }
    func stop() { sink = nil }

    /// `seconds` of a quiet signal at 48 kHz stereo (a common iPhone format).
    func push(seconds: Double, sampleRate: UInt32 = 48_000, channels: UInt32 = 2) {
        let count = Int(Double(sampleRate) * seconds) * Int(channels)
        sink?([Float](repeating: 0.1, count: count), sampleRate, channels)
    }
}

/// Takes every block at once when the test asks.
final class DrainOutput: AudioOutput {
    private var pull: ((Int) -> [Float])?
    private var finished: (() -> Void)?

    func start(sampleRate: Double, pull: @escaping (Int) -> [Float], finished: @escaping () -> Void) throws {
        self.pull = pull
        self.finished = finished
    }

    func stop() { pull = nil }

    /// Returns how many samples were played.
    func drain() -> Int {
        var total = 0
        while let pull, case let block = pull(1024), !block.isEmpty {
            total += block.count
        }
        finished?()
        return total
    }
}

@MainActor
final class RecorderModelTests: XCTestCase {
    private func makeSession(_ library: LectioLibrary) throws -> String {
        try library.addSession(courseId: try library.addCourse(name: "Biology"), title: "Cells")
    }

    func testARecordingIsSavedInTheDesktopFormatAndAttached() async throws {
        // Arrange
        let library = try makeLibrary()
        let session = try makeSession(library)
        let source = FakeSource()
        let recorder = RecorderModel(library: library, makeSource: { source }, usesAudioSession: false)
        var finished: String?
        recorder.onFinished = { finished = $0 }

        // Act
        let started = await recorder.start(session: session)
        source.push(seconds: 1.5)
        recorder.tick()
        let elapsed = recorder.elapsedMs
        let duration = try XCTUnwrap(recorder.stop())

        // Assert
        XCTAssertTrue(started)
        XCTAssertEqual(Double(elapsed), 1_500, accuracy: 50)
        XCTAssertEqual(recorder.state, .idle)
        XCTAssertEqual(finished, session)
        let detail = try library.session(sessionId: session)
        XCTAssertEqual(detail.info.durationMs, duration)
        let wav = try Data(contentsOf: URL(fileURLWithPath: try XCTUnwrap(detail.recordingPath)))
        XCTAssertEqual(String(decoding: wav.prefix(4), as: UTF8.self), "RIFF")
        XCTAssertEqual(wav[22], 1, "mono")
        XCTAssertEqual(UInt32(wav[24]) | UInt32(wav[25]) << 8 | UInt32(wav[26]) << 16, 16_000, "16 kHz")
        XCTAssertEqual(wav[34], 16, "16-bit")
    }

    func testAnInterruptionPausesThenCarriesOnInTheSameFile() async throws {
        let library = try makeLibrary()
        let session = try makeSession(library)
        let source = FakeSource()
        let recorder = RecorderModel(library: library, makeSource: { source }, usesAudioSession: false)
        await recorder.start(session: session)
        source.push(seconds: 1)

        recorder.interruptionBegan()
        XCTAssertEqual(recorder.state, .interrupted)
        recorder.interruptionEnded(shouldResume: false)
        XCTAssertEqual(recorder.state, .interrupted, "waits for the user when iOS doesn't suggest resuming")
        recorder.resume()
        XCTAssertEqual(recorder.state, .recording)
        XCTAssertEqual(source.resumed, 1)

        source.push(seconds: 1)
        let duration = try XCTUnwrap(recorder.stop())
        XCTAssertEqual(Double(duration), 2_000, accuracy: 50)
    }

    func testAnInputThatChangesFormatKeepsRecording() async throws {
        let library = try makeLibrary()
        let session = try makeSession(library)
        let source = FakeSource()
        let recorder = RecorderModel(library: library, makeSource: { source }, usesAudioSession: false)
        await recorder.start(session: session)
        source.push(seconds: 1, sampleRate: 48_000, channels: 2)
        source.push(seconds: 1, sampleRate: 16_000, channels: 1)
        let duration = try XCTUnwrap(recorder.stop())
        XCTAssertEqual(Double(duration), 2_000, accuracy: 50)
    }

    func testALectureWithARecordingIsNeverRecordedOver() async throws {
        let library = try makeLibrary()
        let session = try makeSession(library)
        let source = FakeSource()
        let recorder = RecorderModel(library: library, makeSource: { source }, usesAudioSession: false)
        await recorder.start(session: session)
        source.push(seconds: 0.5)
        recorder.stop()

        let again = await recorder.start(session: session)

        XCTAssertFalse(again)
        XCTAssertEqual(recorder.problem, "This lecture already has a recording.")
    }
}

@MainActor
final class PlayerModelTests: XCTestCase {
    func testPlayingFromANoteStartsThereAndRunsToTheEnd() async throws {
        let library = try makeLibrary()
        let session = try library.addSession(courseId: try library.addCourse(name: "Biology"), title: "Cells")
        let source = FakeSource()
        let recorder = RecorderModel(library: library, makeSource: { source }, usesAudioSession: false)
        await recorder.start(session: session)
        source.push(seconds: 2)
        recorder.stop()
        let path = try XCTUnwrap(try library.session(sessionId: session).recordingPath)
        let output = DrainOutput()
        let player = PlayerModel(output: output, usesAudioSession: false)

        player.play(session: session, path: path, from: 1_000)
        XCTAssertTrue(player.isPlaying)
        XCTAssertEqual(player.positionMs, 1_000)
        let played = output.drain()

        XCTAssertEqual(Double(played), 16_000, accuracy: 1_600, "one second left at normal speed")
        XCTAssertFalse(player.isPlaying)
        XCTAssertEqual(player.positionMs, player.durationMs)
    }

    func testDoubleSpeedPlaysTheSameAudioInHalfTheTime() async throws {
        let library = try makeLibrary()
        let session = try library.addSession(courseId: try library.addCourse(name: "Biology"), title: "Cells")
        let source = FakeSource()
        let recorder = RecorderModel(library: library, makeSource: { source }, usesAudioSession: false)
        await recorder.start(session: session)
        source.push(seconds: 2)
        recorder.stop()
        let path = try XCTUnwrap(try library.session(sessionId: session).recordingPath)
        let output = DrainOutput()
        let player = PlayerModel(output: output, usesAudioSession: false)

        player.setSpeed(200)
        player.play(session: session, path: path, from: 0)
        let played = output.drain()

        XCTAssertEqual(Double(played), 16_000, accuracy: 2_400, "two seconds of audio in about one")
    }
}

final class FormattingTests: XCTestCase {
    func testClockTimes() {
        XCTAssertEqual(TimeText.clock(0), "0:00")
        XCTAssertEqual(TimeText.clock(7_900), "0:07")
        XCTAssertEqual(TimeText.clock(750_000), "12:30")
        XCTAssertEqual(TimeText.clock(3_723_000), "1:02:03")
    }

    func testSpokenTimesForVoiceOver() {
        XCTAssertEqual(TimeText.spoken(0), "0 seconds")
        XCTAssertEqual(TimeText.spoken(65_000), "1 minute, 5 seconds")
    }

    func testErrorsFromTheCoreKeepTheirSentence() {
        XCTAssertEqual(errorText(LectioError.Failed(message: "That item can't be found.")), "That item can't be found.")
    }

    func testTheToneIsStereo48kHzInTenthsOfASecond() {
        XCTAssertEqual(ToneSource().nextBlock().count, 4_800 * 2)
    }
}
