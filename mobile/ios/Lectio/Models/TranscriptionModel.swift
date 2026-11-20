import Foundation

/// Transcribes a lecture on the device with Whisper (the desktop's engine,
/// using Apple's Accelerate for the maths). The first run downloads the
/// model. Segments are saved as each 30-second window finishes, so the
/// transcript fills in while it runs and a stopped job keeps what it has.
@MainActor
final class TranscriptionModel: ObservableObject {
    enum Phase: Equatable {
        case idle
        case downloading(Double)
        case loading
        case transcribing(Double)
        /// Segments stored and seconds taken (shown, so speed on a real
        /// device can be read off the screen).
        case finished(UInt32, Double)
        case failed(String)
    }

    @Published private(set) var phase: Phase = .idle
    /// "tiny", "base" or "small".
    @Published var modelName = "base"
    private var job: JobProgress?

    let models: [ModelInfo] = speechModels()

    var isRunning: Bool {
        switch phase {
        case .downloading, .loading, .transcribing: return true
        default: return false
        }
    }

    var selectedModel: ModelInfo? {
        models.first { $0.name == modelName }
    }

    /// `stored` runs on the main actor each time a window is saved.
    func transcribe(session: String, library: LectioLibrary, store: ModelStore, stored: @escaping () -> Void) async {
        guard !isRunning, let model = selectedModel else { return }
        do {
            if !store.isDownloaded(model) {
                phase = .downloading(0)
                try await store.download(model) { [weak self] fraction in
                    Task { @MainActor in self?.phase = .downloading(fraction) }
                }
            }
            phase = .loading
            let engine = try await store.speechEngine(model)
            phase = .transcribing(0)
            let progress = JobProgress { [weak self] fraction in
                Task { @MainActor in
                    self?.phase = .transcribing(fraction)
                    stored()
                }
            }
            job = progress
            let started = Date()
            let count = try await Task.detached(priority: .userInitiated) {
                try engine.transcribeSession(library: library, sessionId: session, language: nil, listener: progress)
            }.value
            job = nil
            phase = .finished(count, Date().timeIntervalSince(started))
            stored()
        } catch {
            job = nil
            phase = .failed(errorText(error))
        }
    }

    func cancel() {
        job?.cancel()
    }
}
