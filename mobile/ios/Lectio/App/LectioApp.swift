import SwiftUI

/// How the app was started. Normal launches keep the library in
/// Application Support/Lectio, in the same layout as the desktop app.
/// UI tests pass `-LectioFreshStart YES` (an empty library in a temporary
/// folder) and `-LectioTestAudio YES` (a test tone instead of the microphone
/// and a silent clock instead of the speaker, because CI Simulators have no
/// audio hardware).
struct LaunchOptions {
    let dataDirectory: URL
    let usesTestAudio: Bool

    static func current(_ defaults: UserDefaults = .standard) -> LaunchOptions {
        let fresh = defaults.bool(forKey: "LectioFreshStart")
        let directory = fresh ? testDirectory() : libraryDirectory()
        return LaunchOptions(dataDirectory: directory, usesTestAudio: defaults.bool(forKey: "LectioTestAudio"))
    }

    static func libraryDirectory() -> URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        return base.appendingPathComponent("Lectio", isDirectory: true)
    }

    private static func testDirectory() -> URL {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent("LectioUITest", isDirectory: true)
        if FileManager.default.fileExists(atPath: directory.path) {
            do {
                try FileManager.default.removeItem(at: directory)
            } catch {
                NSLog("Lectio: could not clear the test library: %@", error.localizedDescription)
            }
        }
        return directory
    }
}

/// The library and the audio, shared by every screen.
@MainActor
final class AppModel: ObservableObject {
    let library: LectioLibrary
    let recorder: RecorderModel
    let player: PlayerModel
    let models: ModelStore
    /// Bumped after every change, so screens that show library data reload.
    @Published private(set) var revision = 0

    init(library: LectioLibrary, options: LaunchOptions) {
        self.library = library
        let testAudio = options.usesTestAudio
        recorder = RecorderModel(
            library: library,
            makeSource: { testAudio ? ToneSource() as AudioSource : MicrophoneSource() },
            usesAudioSession: !testAudio
        )
        player = PlayerModel(output: testAudio ? ClockOutput() as AudioOutput : EngineOutput(), usesAudioSession: !testAudio)
        models = ModelStore(dataDirectory: options.dataDirectory)
        recorder.onFinished = { [weak self] _ in self?.changed() }
    }

    func changed() {
        revision &+= 1
    }
}

/// Opens the library once at launch; if that fails the app says why
/// instead of crashing.
@MainActor
final class Startup: ObservableObject {
    let app: AppModel?
    let problem: String

    init(options: LaunchOptions = .current()) {
        do {
            try FileManager.default.createDirectory(at: options.dataDirectory, withIntermediateDirectories: true)
            let library = try LectioLibrary.open(dataDir: options.dataDirectory.path)
            app = AppModel(library: library, options: options)
            problem = ""
        } catch {
            app = nil
            problem = errorText(error)
        }
    }
}

@main
struct LectioApp: App {
    @StateObject private var startup = Startup()

    var body: some Scene {
        WindowGroup {
            if let app = startup.app {
                RootView(app: app)
                    .environmentObject(app)
                    .environmentObject(app.recorder)
                    .environmentObject(app.player)
                    .environmentObject(app.models)
            } else {
                VStack(spacing: 12) {
                    Image(systemName: "exclamationmark.triangle")
                        .font(.largeTitle)
                        .accessibilityHidden(true)
                    Text("Lectio can't open your notes")
                        .font(.headline)
                    Text(startup.problem)
                        .font(.body)
                        .multilineTextAlignment(.center)
                }
                .padding()
            }
        }
    }
}
