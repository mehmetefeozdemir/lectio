import PhotosUI
import SwiftUI

/// The lecture's notes in time order; each one plays the recording from
/// the moment it was written. The composer is at the bottom.
struct NoteFeedView: View {
    @ObservedObject var model: SessionModel
    let offsetNow: () -> UInt64
    let playFrom: (UInt64) -> Void
    @State private var editing: NoteInfo?
    @State private var editText = ""

    var body: some View {
        VStack(spacing: 0) {
            ScrollViewReader { proxy in
                List {
                    ForEach(model.notes, id: \.id) { note in
                        NoteRow(note: note, canPlay: model.recordingPath != nil, playFrom: playFrom)
                            .id(note.id)
                            .contextMenu { menu(for: note) }
                            .swipeActions {
                                Button(role: .destructive) { model.delete(note) } label: {
                                    Label("Delete", systemImage: "trash")
                                }
                            }
                    }
                }
                .listStyle(.plain)
                .scrollDismissesKeyboard(.interactively)
                .overlay {
                    if model.notes.isEmpty {
                        Text("No notes yet. Each note you write keeps the moment of the recording it belongs to.")
                            .foregroundStyle(.secondary)
                            .multilineTextAlignment(.center)
                            .padding()
                    }
                }
                .onChange(of: model.notes.count) { _ in
                    if let last = model.notes.last?.id {
                        withAnimation { proxy.scrollTo(last, anchor: .bottom) }
                    }
                }
            }
            NoteComposer(model: model, offsetNow: offsetNow)
        }
        .alert("Edit note", isPresented: Binding(get: { editing != nil }, set: { if !$0 { editing = nil } })) {
            TextField("Note", text: $editText)
            Button("Save") {
                if let note = editing { model.edit(note, text: editText) }
                editing = nil
            }
            Button("Cancel", role: .cancel) { editing = nil }
        }
    }

    @ViewBuilder
    private func menu(for note: NoteInfo) -> some View {
        ForEach(NoteLabel.all, id: \.self) { label in
            Button { model.toggle(label, on: note) } label: {
                Label(label.title, systemImage: note.labels.contains(label) ? "checkmark" : label.symbol)
            }
        }
        Divider()
        if note.kind == .text {
            Button {
                editText = note.body
                editing = note
            } label: { Label("Edit", systemImage: "pencil") }
        }
        if let slide = model.currentSlide, note.kind != .slide {
            Button { model.link(note, to: slide) } label: {
                Label("Link to slide \(slide.page)", systemImage: "link")
            }
        }
        if note.slide != nil, note.kind != .slide {
            Button { model.link(note, to: nil) } label: { Label("Unlink from the slide", systemImage: "link.badge.plus") }
        }
        Button(role: .destructive) { model.delete(note) } label: { Label("Delete", systemImage: "trash") }
    }
}

struct NoteRow: View {
    let note: NoteInfo
    let canPlay: Bool
    let playFrom: (UInt64) -> Void

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Button { playFrom(note.offsetMs) } label: {
                Text(TimeText.clock(note.offsetMs))
                    .font(.callout.monospacedDigit())
            }
            .buttonStyle(.bordered)
            .disabled(!canPlay)
            .accessibilityLabel("Play from \(TimeText.spoken(note.offsetMs))")
            .accessibilityIdentifier("note.play")
            VStack(alignment: .leading, spacing: 6) {
                content
                if !note.labels.isEmpty {
                    HStack(spacing: 6) {
                        ForEach(note.labels, id: \.self) { LabelChip(label: $0) }
                    }
                    .accessibilityElement(children: .ignore)
                    .accessibilityLabel("Labels: " + note.labels.map(\.title).joined(separator: ", "))
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.vertical, 4)
    }

    @ViewBuilder
    private var content: some View {
        switch note.kind {
        case .text:
            if let photo = note.photo {
                PhotoThumbnail(photo: photo, caption: note.body)
            } else {
                Text(note.body)
                    .font(note.labels.contains(.heading) ? .headline : .body)
                    .textSelection(.enabled)
            }
        case .formula:
            FormulaImage(latex: note.body)
        case .slide:
            Label(note.body.isEmpty ? "Slide \(note.slide?.page ?? 0)" : note.body, systemImage: "rectangle.on.rectangle")
                .foregroundStyle(.secondary)
        }
    }
}

/// A photo note's picture, scaled down off the main thread.
struct PhotoThumbnail: View {
    static let maxSide: CGFloat = 600
    let photo: PhotoInfo
    let caption: String
    @State private var image: UIImage?

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Group {
                if let image {
                    Image(uiImage: image)
                        .resizable()
                        .scaledToFit()
                        .clipShape(RoundedRectangle(cornerRadius: 8))
                } else {
                    RoundedRectangle(cornerRadius: 8)
                        .fill(Color(uiColor: .secondarySystemFill))
                        .aspectRatio(CGFloat(max(photo.width, 1)) / CGFloat(max(photo.height, 1)), contentMode: .fit)
                }
            }
            .frame(maxHeight: 240)
            .accessibilityLabel(photo.isSketch ? "Sketch" : "Photo")
            if !caption.isEmpty {
                Text(caption)
            }
        }
        .task(id: photo.path) {
            let path = photo.path
            let side = Self.maxSide
            image = await Task.detached(priority: .utility) {
                UIImage(contentsOfFile: path)?.preparingThumbnail(of: CGSize(width: side, height: side))
            }.value
        }
    }
}

/// A formula drawn by the core (the same renderer as the desktop and the
/// exports), in light or dark ink, at the reader's text size.
struct FormulaImage: View {
    let latex: String
    @Environment(\.colorScheme) private var scheme
    @Environment(\.displayScale) private var displayScale
    @ScaledMetric(relativeTo: .body) private var emSize: CGFloat = 20
    @State private var image: UIImage?
    @State private var problem: String?

    var body: some View {
        Group {
            if let image {
                Image(uiImage: image)
                    .resizable()
                    .scaledToFit()
                    .frame(width: image.size.width, height: image.size.height)
            } else if let problem {
                Text(problem).foregroundStyle(.red).font(.footnote)
            } else {
                Text(latex).font(.body.monospaced())
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Formula: \(latex)")
        .task(id: "\(latex)|\(scheme == .dark)|\(emSize)|\(displayScale)") { render() }
    }

    private func render() {
        if let message = checkFormula(latex: latex) {
            problem = message
            image = nil
            return
        }
        do {
            let png = try renderFormulaPng(latex: latex, sizePx: Float(emSize * displayScale), dark: scheme == .dark)
            image = UIImage(data: png, scale: displayScale)
            problem = nil
        } catch {
            problem = errorText(error)
        }
    }
}

/// Text, labels, and a menu for formulas and photos.
struct NoteComposer: View {
    @ObservedObject var model: SessionModel
    let offsetNow: () -> UInt64
    @State private var writingFormula = false
    @State private var takingPhoto = false
    @State private var choosingPhoto = false
    @State private var pickedPhoto: PhotosPickerItem?
    /// The moment the formula or photo was started, so it lands where the
    /// lecturer was, not where they are once it's finished.
    @State private var startedAt: UInt64 = 0

    var body: some View {
        VStack(spacing: 8) {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 6) {
                    ForEach(NoteLabel.all, id: \.self) { label in
                        let on = model.draftLabels.contains(label)
                        Button { model.toggleDraftLabel(label) } label: {
                            LabelChip(label: label, selected: on)
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel(label.title)
                        .accessibilityAddTraits(on ? .isSelected : [])
                    }
                }
            }
            HStack(alignment: .bottom, spacing: 8) {
                Menu {
                    Button { begin { writingFormula = true } } label: { Label("Formula", systemImage: "function") }
                    Button { begin { takingPhoto = true } } label: { Label("Take a photo", systemImage: "camera") }
                        .disabled(!UIImagePickerController.isSourceTypeAvailable(.camera))
                    Button { begin { choosingPhoto = true } } label: { Label("Choose a photo", systemImage: "photo") }
                } label: {
                    Image(systemName: "plus.circle")
                        .font(.title2)
                        .frame(minWidth: 44, minHeight: 44)
                }
                .accessibilityLabel("Add a formula or photo")
                TextField("Write a note", text: $model.draft, axis: .vertical)
                    .lineLimit(1...5)
                    .textFieldStyle(.roundedBorder)
                    .accessibilityIdentifier("composer.text")
                    .onChange(of: model.draft) { _ in model.draftEdited(at: offsetNow()) }
                Button {
                    model.addNote(at: offsetNow())
                } label: {
                    Image(systemName: "arrow.up.circle.fill")
                        .font(.title2)
                        .frame(minWidth: 44, minHeight: 44)
                }
                .disabled(model.draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                .accessibilityLabel("Add note")
                .accessibilityIdentifier("composer.add")
            }
        }
        .padding(.horizontal)
        .padding(.vertical, 8)
        .background(.bar)
        .sheet(isPresented: $writingFormula) {
            FormulaEditor { model.addFormula($0, at: startedAt) }
        }
        .fullScreenCover(isPresented: $takingPhoto) {
            CameraPicker { image in
                if let image { model.addPhoto(image, caption: "", at: startedAt) }
            }
            .ignoresSafeArea()
        }
        .photosPicker(isPresented: $choosingPhoto, selection: $pickedPhoto, matching: .images)
        .onChange(of: pickedPhoto) { item in
            guard let item else { return }
            Task {
                do {
                    if let data = try await item.loadTransferable(type: Data.self), let image = UIImage(data: data) {
                        model.addPhoto(image, caption: "", at: startedAt)
                    } else {
                        model.problem = "That picture couldn't be read."
                    }
                } catch {
                    model.problem = errorText(error)
                }
                pickedPhoto = nil
            }
        }
    }

    private func begin(_ open: () -> Void) {
        startedAt = offsetNow()
        open()
    }
}

/// LaTeX with a live preview drawn by the core, and buttons for the
/// pieces that are awkward to type on a phone keyboard.
struct FormulaEditor: View {
    static let snippets: [(label: String, insert: String)] = [
        ("a/b", "\\frac{}{}"), ("x²", "^{}"), ("xᵢ", "_{}"), ("√", "\\sqrt{}"),
        ("Σ", "\\sum_{}^{}"), ("∫", "\\int_{}^{}"), ("α", "\\alpha"), ("π", "\\pi"),
    ]

    let add: (String) -> Bool
    @Environment(\.dismiss) private var dismiss
    @State private var latex = ""

    var body: some View {
        NavigationStack {
            Form {
                Section("LaTeX") {
                    TextField("e.g. E = mc^2", text: $latex, axis: .vertical)
                        .font(.body.monospaced())
                        .autocorrectionDisabled()
                        .textInputAutocapitalization(.never)
                        .accessibilityIdentifier("formula.text")
                    ScrollView(.horizontal, showsIndicators: false) {
                        HStack {
                            ForEach(Self.snippets, id: \.insert) { snippet in
                                Button(snippet.label) { latex += snippet.insert }
                                    .buttonStyle(.bordered)
                                    .accessibilityLabel("Insert \(snippet.insert)")
                            }
                        }
                    }
                }
                Section("Preview") {
                    if latex.trimmingCharacters(in: .whitespaces).isEmpty {
                        Text("Type a formula to see it here.").foregroundStyle(.secondary)
                    } else {
                        FormulaImage(latex: latex)
                    }
                }
            }
            .navigationTitle("Formula")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Add") {
                        if add(latex) { dismiss() }
                    }
                    .disabled(latex.trimmingCharacters(in: .whitespaces).isEmpty || checkFormula(latex: latex) != nil)
                }
            }
        }
    }
}

/// The camera (UIKit's picker; SwiftUI has none on iOS 16).
struct CameraPicker: UIViewControllerRepresentable {
    let done: (UIImage?) -> Void

    func makeUIViewController(context: Context) -> UIImagePickerController {
        let picker = UIImagePickerController()
        picker.sourceType = .camera
        picker.delegate = context.coordinator
        return picker
    }

    func updateUIViewController(_ controller: UIImagePickerController, context: Context) {}

    func makeCoordinator() -> Coordinator { Coordinator(done: done) }

    final class Coordinator: NSObject, UIImagePickerControllerDelegate, UINavigationControllerDelegate {
        let done: (UIImage?) -> Void

        init(done: @escaping (UIImage?) -> Void) { self.done = done }

        func imagePickerController(_ picker: UIImagePickerController, didFinishPickingMediaWithInfo info: [UIImagePickerController.InfoKey: Any]) {
            done(info[.originalImage] as? UIImage)
            picker.dismiss(animated: true)
        }

        func imagePickerControllerDidCancel(_ picker: UIImagePickerController) {
            done(nil)
            picker.dismiss(animated: true)
        }
    }
}
