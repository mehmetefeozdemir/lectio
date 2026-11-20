import XCTest
@testable import Lectio

/// Times Whisper tiny and base through the same Swift → Rust path the app
/// uses. Skipped unless a folder with the models and a WAV is given:
///
///     TEST_RUNNER_LECTIO_BENCH_DIR=/path xcodebuild test ... \
///         -only-testing:LectioTests/WhisperBenchmarkTests
///
/// The folder holds `whisper-tiny/`, `whisper-base/` (the files the app
/// downloads) and `speech.wav` (16 kHz mono). Simulator numbers use the
/// Mac's CPU; a real iPhone is needed for real numbers.
final class WhisperBenchmarkTests: XCTestCase {
    func testTranscriptionSpeed() throws {
        guard let folder = ProcessInfo.processInfo.environment["LECTIO_BENCH_DIR"] else {
            throw XCTSkip("Set TEST_RUNNER_LECTIO_BENCH_DIR to run the Whisper benchmark.")
        }
        let wav = (folder as NSString).appendingPathComponent("speech.wav")
        var lines: [String] = []
        for name in ["tiny", "base"] {
            let models = (folder as NSString).appendingPathComponent("whisper-\(name)")
            guard FileManager.default.fileExists(atPath: models) else { continue }
            let loadStart = Date()
            let engine = try SpeechEngine.load(modelDir: models, modelName: name)
            let loadSeconds = Date().timeIntervalSince(loadStart)
            let result = try engine.transcribeWav(path: wav, language: "en")
            let speed = result.audioSeconds / max(result.seconds, 0.001)
            let line = String(
                format: "whisper-%@: load %.2f s, %.1f s of audio in %.2f s (%.1fx real time) | %@",
                name, loadSeconds, result.audioSeconds, result.seconds, speed, result.text
            )
            lines.append(line)
            print("BENCH " + line)
            XCTAssertFalse(result.text.isEmpty)
        }
        XCTAssertFalse(lines.isEmpty, "no whisper-tiny or whisper-base folder in \(folder)")
        add(XCTAttachment(string: lines.joined(separator: "\n")))
    }
}
