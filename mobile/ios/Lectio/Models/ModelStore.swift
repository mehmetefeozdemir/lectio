import Foundation

/// A Swift `ProgressListener` for the Rust engines: reports progress
/// (0...1) and lets the screen stop a long job.
final class JobProgress: ProgressListener, @unchecked Sendable {
    private let lock = NSLock()
    private var cancelled = false
    private let report: @Sendable (Double) -> Void

    init(report: @escaping @Sendable (Double) -> Void) {
        self.report = report
    }

    func progress(done: UInt64, total: UInt64) {
        report(total == 0 ? 0 : min(1, Double(done) / Double(total)))
    }

    func shouldContinue() -> Bool {
        lock.lock()
        defer { lock.unlock() }
        return !cancelled
    }

    func cancel() {
        lock.lock()
        cancelled = true
        lock.unlock()
    }
}

enum DownloadProblem: LocalizedError {
    case badAddress(String)
    case server(Int, String)
    case damaged(String)

    var errorDescription: String? {
        switch self {
        case let .badAddress(name): return "The download address for \(name) isn't valid."
        case let .server(code, name): return "The server answered \(code) for \(name). Try again later."
        case let .damaged(name): return "The download of \(name) was damaged (its checksum didn't match). Try again."
        }
    }
}

/// The on-device models (Whisper for transcripts, the study model for
/// outlines and quizzes): downloaded once with URLSession into
/// `models/<folder>/` in the desktop's layout, checked by the core
/// (size and SHA-256), and loaded once per launch.
@MainActor
final class ModelStore: ObservableObject {
    let modelsDirectory: URL
    private var speechEngines: [String: SpeechEngine] = [:]
    private var studyEngines: [String: StudyEngine] = [:]

    init(dataDirectory: URL) {
        modelsDirectory = dataDirectory.appendingPathComponent("models", isDirectory: true)
    }

    func folder(for model: ModelInfo) -> URL {
        modelsDirectory.appendingPathComponent(model.dirName, isDirectory: true)
    }

    /// Every file is there with the right size (the checksum was checked
    /// when it was downloaded).
    func isDownloaded(_ model: ModelInfo) -> Bool {
        model.files.allSatisfy { file in
            let path = folder(for: model).appendingPathComponent(file.name).path
            let size = (try? FileManager.default.attributesOfItem(atPath: path)[.size] as? NSNumber)?.uint64Value
            return size == file.bytes
        }
    }

    /// Downloads the missing files; `progress` gets 0...1 over all of them.
    func download(_ model: ModelInfo, progress: @escaping @Sendable (Double) -> Void) async throws {
        let folder = folder(for: model)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let total = Double(max(model.totalBytes, 1))
        var done: UInt64 = 0
        for file in model.files {
            let target = folder.appendingPathComponent(file.name)
            let size = (try? FileManager.default.attributesOfItem(atPath: target.path)[.size] as? NSNumber)?.uint64Value
            if size != file.bytes {
                let before = Double(done)
                let bytes = Double(file.bytes)
                try await fetch(file, to: target) { fraction in progress((before + fraction * bytes) / total) }
            }
            done += file.bytes
            progress(Double(done) / total)
        }
    }

    func speechEngine(_ model: ModelInfo) async throws -> SpeechEngine {
        if let engine = speechEngines[model.name] { return engine }
        let path = folder(for: model).path
        let name = model.name
        let engine = try await Task.detached(priority: .userInitiated) {
            try SpeechEngine.load(modelDir: path, modelName: name)
        }.value
        speechEngines[model.name] = engine
        return engine
    }

    func studyEngine(_ model: ModelInfo) async throws -> StudyEngine {
        if let engine = studyEngines[model.name] { return engine }
        let path = folder(for: model).path
        let name = model.name
        let engine = try await Task.detached(priority: .userInitiated) {
            try StudyEngine.load(modelDir: path, modelName: name)
        }.value
        studyEngines[model.name] = engine
        return engine
    }

    /// Downloads one file next to its target, checks it, then moves it in,
    /// so a half-finished download is never mistaken for the model.
    private func fetch(_ file: FileInfo, to target: URL, progress: @escaping @Sendable (Double) -> Void) async throws {
        guard let url = URL(string: file.url) else { throw DownloadProblem.badAddress(file.name) }
        let partial = target.appendingPathExtension("partial")
        try await Self.download(url, name: file.name, to: partial, progress: progress)
        let path = partial.path
        let ok = await Task.detached(priority: .utility) {
            verifyDownload(path: path, bytes: file.bytes, sha256: file.sha256)
        }.value
        guard ok else {
            try? FileManager.default.removeItem(at: partial)
            throw DownloadProblem.damaged(file.name)
        }
        if FileManager.default.fileExists(atPath: target.path) {
            try FileManager.default.removeItem(at: target)
        }
        try FileManager.default.moveItem(at: partial, to: target)
    }

    private static func download(_ url: URL, name: String, to destination: URL, progress: @escaping @Sendable (Double) -> Void) async throws {
        var observation: NSKeyValueObservation?
        defer { observation?.invalidate() }
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            let task = URLSession.shared.downloadTask(with: url) { temporary, response, error in
                if let error {
                    continuation.resume(throwing: error)
                    return
                }
                let status = (response as? HTTPURLResponse)?.statusCode ?? 0
                guard let temporary, status == 200 else {
                    continuation.resume(throwing: DownloadProblem.server(status, name))
                    return
                }
                // URLSession deletes `temporary` when this returns, so move it now.
                do {
                    if FileManager.default.fileExists(atPath: destination.path) {
                        try FileManager.default.removeItem(at: destination)
                    }
                    try FileManager.default.moveItem(at: temporary, to: destination)
                    continuation.resume()
                } catch {
                    continuation.resume(throwing: error)
                }
            }
            observation = task.progress.observe(\.fractionCompleted) { value, _ in
                progress(value.fractionCompleted)
            }
            task.resume()
        }
    }
}
