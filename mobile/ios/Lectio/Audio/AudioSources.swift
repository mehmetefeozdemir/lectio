import AVFoundation

/// Interleaved samples (-1...1) as the input delivered them, with its
/// sample rate and channel count. Rust converts them to 16 kHz mono.
typealias SampleSink = (_ samples: [Float], _ sampleRate: UInt32, _ channels: UInt32) -> Void

/// Where recorded sound comes from: the microphone, or a test tone.
protocol AudioSource: AnyObject {
    /// Starts delivering samples to `sink`, on any thread.
    func start(_ sink: @escaping SampleSink) throws
    /// Starts again after an interruption (a call, Siri) ended.
    func resume() throws
    func stop()
}

enum AudioProblem: LocalizedError {
    case noInput

    var errorDescription: String? {
        switch self {
        case .noInput: return "No microphone is available right now."
        }
    }
}

/// The app's audio session. Play-and-record, so a note can be played back
/// without changing modes; the built-in speaker rather than the earpiece;
/// Bluetooth headsets (AirPods) for both directions.
enum AudioSessionSetup {
    static func activate() throws {
        let session = AVAudioSession.sharedInstance()
        try session.setCategory(.playAndRecord, mode: .default, options: bluetoothOptions)
        try session.setActive(true)
    }

    /// iOS 26's SDK (Swift 6.2, Xcode 26) renamed `.allowBluetooth` to
    /// `.allowBluetoothHFP`; the same option, so build with either Xcode.
    private static var bluetoothOptions: AVAudioSession.CategoryOptions {
        #if compiler(>=6.2)
        return [.defaultToSpeaker, .allowBluetoothHFP, .allowBluetoothA2DP]
        #else
        return [.defaultToSpeaker, .allowBluetooth, .allowBluetoothA2DP]
        #endif
    }

    static func requestMicrophone() async -> Bool {
        if #available(iOS 17, *) {
            return await AVAudioApplication.requestRecordPermission()
        }
        return await withCheckedContinuation { continuation in
            AVAudioSession.sharedInstance().requestRecordPermission { continuation.resume(returning: $0) }
        }
    }

    /// "iPhone Microphone", "AirPods Pro", ...
    static var inputName: String {
        AVAudioSession.sharedInstance().currentRoute.inputs.first?.portName ?? ""
    }
}

/// The microphone, through AVAudioEngine's input node. When the hardware
/// changes (AirPods connect or disconnect) the engine stops and posts a
/// configuration change; the tap goes back on with the new format, and the
/// Rust recorder starts a new converter for it.
final class MicrophoneSource: AudioSource {
    static let tapFrames: AVAudioFrameCount = 4096

    private let engine = AVAudioEngine()
    private var sink: SampleSink?
    private var observer: NSObjectProtocol?

    func start(_ sink: @escaping SampleSink) throws {
        self.sink = sink
        observer = NotificationCenter.default.addObserver(
            forName: .AVAudioEngineConfigurationChange, object: engine, queue: .main
        ) { [weak self] _ in
            do {
                try self?.resume()
            } catch {
                NSLog("Lectio: the microphone did not restart after a route change: %@", error.localizedDescription)
            }
        }
        try resume()
    }

    func resume() throws {
        guard let sink else { return }
        let input = engine.inputNode
        input.removeTap(onBus: 0)
        let format = input.outputFormat(forBus: 0)
        guard format.sampleRate > 0, format.channelCount > 0 else { throw AudioProblem.noInput }
        input.installTap(onBus: 0, bufferSize: Self.tapFrames, format: format) { buffer, _ in
            Self.deliver(buffer, to: sink)
        }
        engine.prepare()
        try engine.start()
    }

    func stop() {
        if let observer {
            NotificationCenter.default.removeObserver(observer)
        }
        observer = nil
        engine.inputNode.removeTap(onBus: 0)
        engine.stop()
        sink = nil
    }

    /// Interleaves the tap's buffer (AVAudioEngine gives one array per channel).
    static func deliver(_ buffer: AVAudioPCMBuffer, to sink: SampleSink) {
        guard let channelData = buffer.floatChannelData else { return }
        let frames = Int(buffer.frameLength)
        let channels = Int(buffer.format.channelCount)
        guard frames > 0, channels > 0 else { return }
        var samples = [Float](repeating: 0, count: frames * channels)
        if buffer.format.isInterleaved {
            for i in 0..<(frames * channels) {
                samples[i] = channelData[0][i]
            }
        } else {
            for channel in 0..<channels {
                let data = channelData[channel]
                for frame in 0..<frames {
                    samples[frame * channels + channel] = data[frame]
                }
            }
        }
        sink(samples, UInt32(buffer.format.sampleRate), UInt32(channels))
    }
}

/// A steady 440 Hz tone, 48 kHz stereo, in 100 ms buffers: for UI tests and
/// machines without a microphone. 48 kHz stereo on purpose, so Rust still
/// converts it to 16 kHz mono like a real microphone's input.
final class ToneSource: AudioSource {
    static let sampleRate: UInt32 = 48_000
    static let channels: UInt32 = 2
    static let blockMs = 100
    static let frequency: Float = 440
    static let level: Float = 0.25

    private let queue = DispatchQueue(label: "org.lectio.tone")
    private var timer: DispatchSourceTimer?
    private var phase: Float = 0

    func start(_ sink: @escaping SampleSink) throws {
        let timer = DispatchSource.makeTimerSource(queue: queue)
        timer.schedule(deadline: .now(), repeating: .milliseconds(Self.blockMs))
        timer.setEventHandler { [weak self] in
            guard let self else { return }
            sink(self.nextBlock(), Self.sampleRate, Self.channels)
        }
        self.timer = timer
        timer.resume()
    }

    func resume() throws {}

    func stop() {
        timer?.cancel()
        timer = nil
    }

    /// One block of the tone, the same sample in both channels.
    func nextBlock() -> [Float] {
        let frames = Int(Self.sampleRate) * Self.blockMs / 1000
        let step = 2 * Float.pi * Self.frequency / Float(Self.sampleRate)
        var samples = [Float](repeating: 0, count: frames * Int(Self.channels))
        for frame in 0..<frames {
            let value = Self.level * sin(phase)
            phase = (phase + step).truncatingRemainder(dividingBy: 2 * Float.pi)
            for channel in 0..<Int(Self.channels) {
                samples[frame * Int(Self.channels) + channel] = value
            }
        }
        return samples
    }
}
