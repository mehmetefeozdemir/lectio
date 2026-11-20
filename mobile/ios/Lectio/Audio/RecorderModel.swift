import AVFoundation
import Foundation

/// A failure seen on the audio thread, picked up on the main thread.
final class FailureBox: @unchecked Sendable {
    private let lock = NSLock()
    private var message: String?

    func record(_ text: String) {
        lock.lock()
        defer { lock.unlock() }
        if message == nil { message = text }
    }

    func take() -> String? {
        lock.lock()
        defer { lock.unlock() }
        let taken = message
        message = nil
        return taken
    }
}

/// Records one lecture at a time. Samples go straight from the audio thread
/// into Rust, which writes the desktop's WAV; the elapsed time shown (and
/// stamped on new notes) is what Rust has written, so a pause for a phone
/// call never leaves a gap between notes and audio.
///
/// Interruptions: a call or Siri stops the input; the recording waits and
/// carries on in the same file when iOS says the interruption ended (or the
/// user taps Resume). Route changes (AirPods in or out) are handled by the
/// source. Recording continues with the screen locked (background audio).
@MainActor
final class RecorderModel: ObservableObject {
    enum State: Equatable {
        case idle
        case recording
        case interrupted
    }

    static let tickNanoseconds: UInt64 = 200_000_000

    @Published private(set) var state: State = .idle
    @Published private(set) var sessionId: String?
    @Published private(set) var elapsedMs: UInt64 = 0
    @Published private(set) var inputName = ""
    @Published var problem: String?

    /// Called with the session id after a recording is saved.
    var onFinished: ((String) -> Void)?

    private let library: LectioLibrary
    private let makeSource: () -> AudioSource
    private let usesAudioSession: Bool
    private let failures = FailureBox()
    private var source: AudioSource?
    private var recorder: AudioRecorder?
    private var ticker: Task<Void, Never>?
    private var observers: [NSObjectProtocol] = []

    init(library: LectioLibrary, makeSource: @escaping () -> AudioSource, usesAudioSession: Bool) {
        self.library = library
        self.makeSource = makeSource
        self.usesAudioSession = usesAudioSession
    }

    func isRecording(_ session: String) -> Bool {
        sessionId == session && state != .idle
    }

    /// Starts recording `session`. A session keeps one recording, so this
    /// refuses a session that already has one rather than overwrite it.
    @discardableResult
    func start(session: String) async -> Bool {
        guard state == .idle else {
            problem = "Lectio is already recording another lecture. Stop it first."
            return false
        }
        do {
            guard try library.session(sessionId: session).recordingPath == nil else {
                problem = "This lecture already has a recording."
                return false
            }
            if usesAudioSession {
                guard await AudioSessionSetup.requestMicrophone() else {
                    problem = "Lectio can't use the microphone. Allow it in Settings > Privacy & Security > Microphone."
                    return false
                }
                try AudioSessionSetup.activate()
                inputName = AudioSessionSetup.inputName
            }
            let recorder = try AudioRecorder.start(path: try library.recordingPath(sessionId: session))
            let source = makeSource()
            let failures = self.failures
            try source.start { samples, sampleRate, channels in
                do {
                    try recorder.push(samples: samples, sampleRate: sampleRate, channels: channels)
                } catch {
                    failures.record(errorText(error))
                }
            }
            self.recorder = recorder
            self.source = source
            sessionId = session
            elapsedMs = 0
            state = .recording
            startTicking()
            observeAudioSession()
            return true
        } catch {
            problem = errorText(error)
            return false
        }
    }

    /// Stops and saves; returns the recording's length.
    @discardableResult
    func stop() -> UInt64? {
        guard let recorder, let session = sessionId else { return nil }
        source?.stop()
        defer { reset() }
        do {
            let duration = try recorder.finish()
            try library.attachRecording(sessionId: session, durationMs: duration)
            elapsedMs = duration
            onFinished?(session)
            return duration
        } catch {
            problem = errorText(error)
            return nil
        }
    }

    /// A call or Siri took the microphone.
    func interruptionBegan() {
        guard state == .recording else { return }
        state = .interrupted
    }

    /// The interruption is over; iOS suggests whether to carry on.
    func interruptionEnded(shouldResume: Bool) {
        guard state == .interrupted, shouldResume else { return }
        resume()
    }

    /// Carries on in the same file after an interruption.
    func resume() {
        guard state == .interrupted, let source else { return }
        do {
            if usesAudioSession {
                try AudioSessionSetup.activate()
            }
            try source.resume()
            state = .recording
        } catch {
            problem = "The recording couldn't resume: \(errorText(error)) Tap Stop to save what was recorded."
        }
    }

    /// Re-reads how much has been written (also called by tests).
    func tick() {
        guard let recorder else { return }
        elapsedMs = recorder.durationMs()
        if let failure = failures.take() {
            problem = failure
            stop()
        }
    }

    private func startTicking() {
        ticker?.cancel()
        ticker = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(nanoseconds: Self.tickNanoseconds)
                self?.tick()
            }
        }
    }

    private func reset() {
        ticker?.cancel()
        ticker = nil
        observers.forEach(NotificationCenter.default.removeObserver)
        observers = []
        recorder = nil
        source = nil
        sessionId = nil
        state = .idle
        _ = failures.take()
    }

    private func observeAudioSession() {
        guard usesAudioSession else { return }
        let center = NotificationCenter.default
        observers.append(center.addObserver(forName: AVAudioSession.interruptionNotification, object: nil, queue: .main) { [weak self] note in
            let info = note.userInfo ?? [:]
            let type = (info[AVAudioSessionInterruptionTypeKey] as? UInt).flatMap(AVAudioSession.InterruptionType.init)
            let options = AVAudioSession.InterruptionOptions(rawValue: info[AVAudioSessionInterruptionOptionKey] as? UInt ?? 0)
            Task { @MainActor in
                switch type {
                case .began: self?.interruptionBegan()
                case .ended: self?.interruptionEnded(shouldResume: options.contains(.shouldResume))
                default: break
                }
            }
        })
        observers.append(center.addObserver(forName: AVAudioSession.routeChangeNotification, object: nil, queue: .main) { [weak self] _ in
            Task { @MainActor in self?.inputName = AudioSessionSetup.inputName }
        })
        observers.append(center.addObserver(forName: AVAudioSession.mediaServicesWereResetNotification, object: nil, queue: .main) { [weak self] _ in
            Task { @MainActor in
                self?.problem = "iOS restarted its audio system. Lectio saved the recording up to this point."
                self?.stop()
            }
        })
    }
}
