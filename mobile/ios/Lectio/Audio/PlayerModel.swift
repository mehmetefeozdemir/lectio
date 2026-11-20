import AVFoundation
import Foundation

/// Where played sound goes. `pull` returns the next mono block at
/// `sampleRate`, empty at the end; `finished` runs on the main queue.
protocol AudioOutput: AnyObject {
    func start(sampleRate: Double, pull: @escaping (_ frames: Int) -> [Float], finished: @escaping () -> Void) throws
    func stop()
}

/// The speaker or headphones: an AVAudioPlayerNode fed small blocks from
/// Rust (which does the speed change, ADR-031). A few blocks are queued
/// ahead; each one that finishes playing asks for the next. A generation
/// number drops callbacks from before a seek or stop.
final class EngineOutput: AudioOutput {
    static let blockFrames = 2048
    static let queuedBlocks = 3

    private let engine = AVAudioEngine()
    private let node = AVAudioPlayerNode()
    private let queue = DispatchQueue(label: "org.lectio.playback")
    private var format: AVAudioFormat?
    // Touched only on `queue`.
    private var generation = 0
    private var pending = 0

    init() {
        engine.attach(node)
    }

    func start(sampleRate: Double, pull: @escaping (Int) -> [Float], finished: @escaping () -> Void) throws {
        guard let format = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: sampleRate, channels: 1, interleaved: false) else {
            throw AudioProblem.noInput
        }
        if self.format != format {
            engine.disconnectNodeOutput(node)
            engine.connect(node, to: engine.mainMixerNode, format: format)
            self.format = format
        }
        if !engine.isRunning {
            engine.prepare()
            try engine.start()
        }
        node.stop()
        queue.sync {
            generation += 1
            pending = 0
            let current = generation
            for _ in 0..<Self.queuedBlocks {
                scheduleNext(current, format, pull, finished)
            }
        }
        node.play()
    }

    func stop() {
        queue.sync {
            generation += 1
            pending = 0
        }
        node.stop()
        engine.pause()
    }

    /// On `queue`.
    private func scheduleNext(_ current: Int, _ format: AVAudioFormat, _ pull: @escaping (Int) -> [Float], _ finished: @escaping () -> Void) {
        guard current == generation else { return }
        let samples = pull(Self.blockFrames)
        if samples.isEmpty {
            if pending == 0 {
                generation += 1
                DispatchQueue.main.async(execute: finished)
            }
            return
        }
        guard let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: AVAudioFrameCount(samples.count)),
              let channel = buffer.floatChannelData?[0] else { return }
        buffer.frameLength = AVAudioFrameCount(samples.count)
        samples.withUnsafeBufferPointer { source in
            if let base = source.baseAddress {
                channel.update(from: base, count: samples.count)
            }
        }
        pending += 1
        node.scheduleBuffer(buffer, completionCallbackType: .dataPlayedBack) { [weak self] _ in
            guard let self else { return }
            self.queue.async {
                guard current == self.generation else { return }
                self.pending -= 1
                self.scheduleNext(current, format, pull, finished)
            }
        }
    }
}

/// Plays nothing, but takes blocks at the real-time pace: for UI tests on
/// machines without a sound device, and for unit tests.
final class ClockOutput: AudioOutput {
    static let blockMs = 100

    private let queue = DispatchQueue(label: "org.lectio.clock-output")
    private var timer: DispatchSourceTimer?

    func start(sampleRate: Double, pull: @escaping (Int) -> [Float], finished: @escaping () -> Void) throws {
        stop()
        let frames = Int(sampleRate) * Self.blockMs / 1000
        let timer = DispatchSource.makeTimerSource(queue: queue)
        timer.schedule(deadline: .now(), repeating: .milliseconds(Self.blockMs))
        timer.setEventHandler { [weak timer] in
            if pull(frames).isEmpty {
                timer?.cancel()
                DispatchQueue.main.async(execute: finished)
            }
        }
        self.timer = timer
        timer.resume()
    }

    func stop() {
        timer?.cancel()
        timer = nil
    }
}

/// Plays a session's recording at 50–200 % speed, from any moment.
@MainActor
final class PlayerModel: ObservableObject {
    static let speeds: [UInt16] = [50, 75, 100, 125, 150, 175, 200]
    static let tickNanoseconds: UInt64 = 100_000_000

    @Published private(set) var sessionId: String?
    @Published private(set) var isPlaying = false
    @Published private(set) var positionMs: UInt64 = 0
    @Published private(set) var durationMs: UInt64 = 0
    @Published private(set) var speedPercent: UInt16 = 100
    @Published var problem: String?

    private let output: AudioOutput
    private let usesAudioSession: Bool
    private var player: AudioPlayer?
    private var ticker: Task<Void, Never>?
    private var routeObserver: NSObjectProtocol?

    init(output: AudioOutput, usesAudioSession: Bool) {
        self.output = output
        self.usesAudioSession = usesAudioSession
    }

    func isLoaded(_ session: String) -> Bool {
        sessionId == session && player != nil
    }

    /// Opens a session's recording (without playing), unless another
    /// session is playing right now.
    func prepare(session: String, path: String) {
        guard !isLoaded(session), !isPlaying else { return }
        do {
            let player = try AudioPlayer.open(path: path)
            player.setSpeedPercent(percent: speedPercent)
            self.player = player
            sessionId = session
            durationMs = player.durationMs()
            positionMs = 0
        } catch {
            problem = errorText(error)
        }
    }

    /// Plays `session` from `ms` (a note's moment), or from where it was.
    func play(session: String, path: String, from ms: UInt64? = nil) {
        if !isLoaded(session) {
            stopOutput()
            sessionId = nil
            player = nil
            prepare(session: session, path: path)
        }
        startOutput(from: ms)
    }

    func pause() {
        stopOutput()
        tick()
    }

    func seek(to ms: UInt64) {
        guard let player else { return }
        if isPlaying {
            // Restarting the output drops the blocks queued from the old position.
            startOutput(from: ms)
        } else {
            player.seekMs(positionMs: ms)
            positionMs = ms
        }
    }

    func setSpeed(_ percent: UInt16) {
        speedPercent = percent
        player?.setSpeedPercent(percent: percent)
    }

    func tick() {
        guard let player else { return }
        positionMs = min(player.positionMs(), durationMs)
    }

    private func startOutput(from ms: UInt64?) {
        guard let player else { return }
        if let ms {
            player.seekMs(positionMs: ms)
            positionMs = ms
        } else if player.isFinished() {
            player.seekMs(positionMs: 0)
        }
        do {
            if usesAudioSession {
                try AudioSessionSetup.activate()
                observeRoute()
            }
            try output.start(
                sampleRate: Double(player.sampleRate()),
                pull: { frames in player.nextBlock(frames: UInt32(frames)) },
                finished: { [weak self] in self?.reachedEnd() }
            )
            isPlaying = true
            startTicking()
        } catch {
            problem = "Lectio can't play the recording: \(errorText(error))"
        }
    }

    private func reachedEnd() {
        stopOutput()
        positionMs = durationMs
    }

    private func stopOutput() {
        output.stop()
        isPlaying = false
        ticker?.cancel()
        ticker = nil
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

    /// Headphones unplugged or AirPods taken out: pause, as iOS apps should.
    private func observeRoute() {
        guard routeObserver == nil else { return }
        routeObserver = NotificationCenter.default.addObserver(
            forName: AVAudioSession.routeChangeNotification, object: nil, queue: .main
        ) { [weak self] note in
            let reason = (note.userInfo?[AVAudioSessionRouteChangeReasonKey] as? UInt).flatMap(AVAudioSession.RouteChangeReason.init)
            guard reason == .oldDeviceUnavailable else { return }
            Task { @MainActor in self?.pause() }
        }
    }
}
