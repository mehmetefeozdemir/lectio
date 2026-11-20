import Foundation
import UIKit

/// One lecture: its notes, slides, transcript and study material, and the
/// changes made to it. Every change goes through the core and is saved at
/// once in the desktop's format.
@MainActor
final class SessionModel: ObservableObject {
    /// Photos are stored as JPEG at this quality (HEIC from the camera
    /// can't be read by the exports on every platform).
    static let photoQuality: CGFloat = 0.85
    /// How long a status line ("Note added at 0:12") stays on screen.
    static let statusNanoseconds: UInt64 = 4_000_000_000

    let sessionId: String
    @Published private(set) var detail: SessionDetail?
    @Published var draft = ""
    @Published var draftLabels: [NoteLabel] = []
    /// The slide on screen in the Slides tab, for "link note to this slide".
    @Published var currentSlide: SlideRef?
    @Published var problem: String?
    /// The last thing that happened, shown briefly and read by VoiceOver.
    @Published private(set) var status: String?
    @Published private(set) var isExporting = false

    let library: LectioLibrary
    private let changed: () -> Void
    /// Remembers when typing started (the core's rule, as on the desktop).
    private let draftClock = NoteDraft()

    init(sessionId: String, library: LectioLibrary, changed: @escaping () -> Void = {}) {
        self.sessionId = sessionId
        self.library = library
        self.changed = changed
    }

    var notes: [NoteInfo] { detail?.notes ?? [] }
    var title: String { detail?.info.title ?? "" }
    var recordingPath: String? { detail?.recordingPath }

    func reload() {
        do {
            detail = try library.session(sessionId: sessionId)
        } catch {
            problem = errorText(error)
        }
    }

    func toggleDraftLabel(_ label: NoteLabel) {
        if let index = draftLabels.firstIndex(of: label) {
            draftLabels.remove(at: index)
        } else {
            draftLabels.append(label)
        }
    }

    /// Call after every edit of `draft`, with the recording's moment now.
    func draftEdited(at nowMs: UInt64) {
        draftClock.setText(text: draft, nowMs: nowMs)
    }

    /// Adds the draft as a note, stamped with the moment typing started
    /// (or `nowMs` if it wasn't typed).
    @discardableResult
    func addNote(at nowMs: UInt64) -> Bool {
        let text = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return false }
        let labels = draftLabels
        let offsetMs = draftClock.anchorMs(nowMs: nowMs)
        guard perform({ _ = try library.addNote(sessionId: sessionId, offsetMs: offsetMs, text: text, labels: labels) }) else {
            return false
        }
        draft = ""
        draftLabels = []
        draftClock.setText(text: "", nowMs: nowMs)
        tell("Note added at \(TimeText.clock(offsetMs))")
        return true
    }

    @discardableResult
    func addFormula(_ latex: String, at offsetMs: UInt64) -> Bool {
        let source = latex.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !source.isEmpty else { return false }
        let added = perform { _ = try library.addFormulaNote(sessionId: sessionId, offsetMs: offsetMs, latex: source, labels: []) }
        if added { tell("Formula added at \(TimeText.clock(offsetMs))") }
        return added
    }

    @discardableResult
    func addPhoto(_ image: UIImage, caption: String, at offsetMs: UInt64) -> Bool {
        guard let jpeg = image.jpegData(compressionQuality: Self.photoQuality) else {
            problem = "That picture couldn't be read."
            return false
        }
        let added = perform { _ = try library.addPhotoNote(sessionId: sessionId, offsetMs: offsetMs, bytes: jpeg, caption: caption) }
        if added { tell("Photo added at \(TimeText.clock(offsetMs))") }
        return added
    }

    func toggle(_ label: NoteLabel, on note: NoteInfo) {
        perform { try library.toggleLabel(sessionId: sessionId, noteId: note.id, label: label) }
    }

    func edit(_ note: NoteInfo, text: String) {
        perform { try library.editNoteText(sessionId: sessionId, noteId: note.id, text: text) }
    }

    func delete(_ note: NoteInfo) {
        if perform({ try library.deleteNote(sessionId: sessionId, noteId: note.id) }) {
            tell("Note deleted")
        }
    }

    func rename(to title: String) {
        perform { try library.renameSession(sessionId: sessionId, title: title.trimmingCharacters(in: .whitespacesAndNewlines)) }
    }

    /// Links a note to the slide on screen, or unlinks it (nil).
    func link(_ note: NoteInfo, to slide: SlideRef?) {
        perform { try library.linkNoteToSlide(sessionId: sessionId, noteId: note.id, deckId: slide?.deckId, page: slide?.page ?? 0) }
    }

    /// Imports a PDF picked in Files; returns the new deck's id.
    @discardableResult
    func importSlides(from url: URL) -> String? {
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        do {
            let bytes = try Data(contentsOf: url)
            let deck = try library.importSlides(sessionId: sessionId, fileName: url.lastPathComponent, bytes: bytes)
            reload()
            changed()
            tell("Slides imported")
            return deck
        } catch {
            problem = errorText(error)
            return nil
        }
    }

    /// Puts a slide into the note feed at `offsetMs`.
    func postSlide(_ slide: SlideRef, at offsetMs: UInt64) {
        if perform({ _ = try library.postSlide(sessionId: sessionId, offsetMs: offsetMs, deckId: slide.deckId, page: slide.page) }) {
            tell("Slide \(slide.page) added to the notes")
        }
    }

    func notes(on slide: SlideRef) -> [NoteInfo] {
        notes.filter { $0.slide?.deckId == slide.deckId && $0.slide?.page == slide.page && $0.kind != .slide }
    }

    /// Writes the lecture as a document in a temporary folder, for the
    /// share sheet; the work runs off the main thread.
    func export(_ format: ExportFormat, includeTranscript: Bool) async -> URL? {
        isExporting = true
        defer { isExporting = false }
        let library = self.library
        let session = sessionId
        do {
            let (name, bytes) = try await Task.detached(priority: .userInitiated) {
                (try library.exportFileName(sessionId: session, format: format),
                 try library.export(sessionId: session, format: format, includeTranscript: includeTranscript))
            }.value
            let folder = FileManager.default.temporaryDirectory.appendingPathComponent("Exports", isDirectory: true)
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
            let url = folder.appendingPathComponent(name)
            try bytes.write(to: url, options: .atomic)
            let size = ByteCountFormatter.string(fromByteCount: Int64(bytes.count), countStyle: .file)
            tell("Exported \(name), \(size)")
            return url
        } catch {
            problem = errorText(error)
            return nil
        }
    }

    private func tell(_ text: String) {
        status = text
        announce(text)
        Task { [weak self] in
            try? await Task.sleep(nanoseconds: Self.statusNanoseconds)
            if self?.status == text { self?.status = nil }
        }
    }

    @discardableResult
    private func perform(_ body: () throws -> Void) -> Bool {
        do {
            try body()
            reload()
            changed()
            return true
        } catch {
            problem = errorText(error)
            return false
        }
    }
}
