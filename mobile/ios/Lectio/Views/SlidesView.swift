import PDFKit
import SwiftUI
import UniformTypeIdentifiers

/// The lecture's PDF slides, drawn by PDFKit, with pen and highlighter ink
/// on top (stored by the core in the desktop's format, in page fractions so
/// it lines up at any size) and the notes linked to the page below.
struct SlidesView: View {
    enum Tool: String, CaseIterable, Identifiable {
        case look = "View"
        case pen = "Pen"
        case highlighter = "Highlighter"
        case eraser = "Eraser"

        var id: String { rawValue }
    }

    /// Pages are drawn this many points wide (times the screen scale).
    static let renderWidth: CGFloat = 1000

    @ObservedObject var model: SessionModel
    let offsetNow: () -> UInt64
    @State private var deckId: String?
    @State private var page: UInt32 = 1
    @State private var importing = false
    @State private var document: PDFDocument?
    @State private var pageImage: UIImage?
    @State private var strokes: [InkStrokeInfo] = []
    @State private var drawing: [Point] = []
    @State private var tool: Tool = .look
    @State private var color: InkColor = .red

    private var decks: [DeckInfo] { model.detail?.decks ?? [] }
    private var deck: DeckInfo? { decks.first { $0.id == deckId } ?? decks.first }

    var body: some View {
        Group {
            if let deck {
                VStack(spacing: 8) {
                    pager(deck)
                    pageView
                    inkBar
                    linkedNotes(deck)
                }
                .padding(.horizontal)
            } else {
                VStack(spacing: 12) {
                    Text("No slides yet")
                        .font(.title3.weight(.semibold))
                    Text("Import the lecture's PDF to write on it and link notes to its pages.")
                        .foregroundStyle(.secondary)
                        .multilineTextAlignment(.center)
                    importButton
                }
                .padding()
                .frame(maxHeight: .infinity)
            }
        }
        .fileImporter(isPresented: $importing, allowedContentTypes: [.pdf]) { result in
            switch result {
            case let .success(url):
                if let id = model.importSlides(from: url) {
                    deckId = id
                    page = 1
                }
            case let .failure(error):
                model.problem = errorText(error)
            }
        }
        .task(id: "\(deck?.id ?? "")|\(page)") { loadPage() }
    }

    private var importButton: some View {
        Button { importing = true } label: { Label("Import a PDF", systemImage: "doc.badge.plus") }
            .buttonStyle(.borderedProminent)
    }

    private func pager(_ deck: DeckInfo) -> some View {
        HStack {
            if decks.count > 1 {
                Picker("Deck", selection: Binding(get: { deck.id }, set: { deckId = $0; page = 1 })) {
                    ForEach(decks, id: \.id) { Text($0.title).tag($0.id) }
                }
                .pickerStyle(.menu)
            } else {
                Text(deck.title).font(.subheadline.weight(.semibold)).lineLimit(1)
            }
            Spacer()
            Button { page -= 1 } label: { Image(systemName: "chevron.left").frame(minWidth: 44, minHeight: 44) }
                .disabled(page <= 1)
                .accessibilityLabel("Previous page")
            Text("\(page) / \(deck.pageCount)")
                .font(.subheadline.monospacedDigit())
                .accessibilityLabel("Page \(page) of \(deck.pageCount)")
            Button { page += 1 } label: { Image(systemName: "chevron.right").frame(minWidth: 44, minHeight: 44) }
                .disabled(page >= deck.pageCount)
                .accessibilityLabel("Next page")
            Menu {
                Button { model.postSlide(SlideRef(deckId: deck.id, page: page), at: offsetNow()) } label: {
                    Label("Add this slide to the notes", systemImage: "text.badge.plus")
                }
                Button { importing = true } label: { Label("Import another PDF", systemImage: "doc.badge.plus") }
            } label: {
                Image(systemName: "ellipsis.circle").frame(minWidth: 44, minHeight: 44)
            }
            .accessibilityLabel("Slide actions")
        }
    }

    private var pageView: some View {
        GeometryReader { geometry in
            let size = fitted(in: geometry.size)
            ZStack {
                if let pageImage {
                    Image(uiImage: pageImage)
                        .resizable()
                        .frame(width: size.width, height: size.height)
                } else {
                    ProgressView()
                }
                Canvas { context, canvas in
                    for stroke in strokes {
                        draw(stroke.points, width: stroke.drawnWidth, color: stroke.color.color.opacity(Double(stroke.opacity)), in: canvas, context: &context)
                    }
                    if !drawing.isEmpty, tool == .pen || tool == .highlighter {
                        let highlight = tool == .highlighter
                        draw(drawing, width: highlight ? 0.016 : 0.003, color: color.color.opacity(highlight ? 0.38 : 1), in: canvas, context: &context)
                    }
                }
                .frame(width: size.width, height: size.height)
                .allowsHitTesting(false)
            }
            .frame(width: size.width, height: size.height)
            .contentShape(Rectangle())
            .gesture(inkGesture(size), including: tool == .look ? .subviews : .all)
            .position(x: geometry.size.width / 2, y: geometry.size.height / 2)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel("Slide \(page)")
            .accessibilityValue(strokes.isEmpty ? "" : "\(strokes.count) ink strokes")
        }
        .frame(maxHeight: .infinity)
    }

    private var inkBar: some View {
        HStack {
            Picker("Tool", selection: $tool) {
                ForEach(Tool.allCases) { Text($0.rawValue).tag($0) }
            }
            .pickerStyle(.segmented)
            if tool == .pen || tool == .highlighter {
                Menu {
                    Picker("Colour", selection: $color) {
                        ForEach(InkColor.all, id: \.self) { Text($0.title).tag($0) }
                    }
                } label: {
                    Circle().fill(color.color).frame(width: 28, height: 28)
                        .overlay(Circle().stroke(.secondary, lineWidth: 1))
                        .frame(minWidth: 44, minHeight: 44)
                }
                .accessibilityLabel("Ink colour, \(color.title)")
            }
            Button { changeInk { try $0.undoInk(sessionId: $1, deckId: $2, page: $3) } } label: {
                Image(systemName: "arrow.uturn.backward").frame(minWidth: 44, minHeight: 44)
            }
            .disabled(strokes.isEmpty)
            .accessibilityLabel("Undo ink")
            Button(role: .destructive) { changeInk { try $0.clearInk(sessionId: $1, deckId: $2, page: $3) } } label: {
                Image(systemName: "trash").frame(minWidth: 44, minHeight: 44)
            }
            .disabled(strokes.isEmpty)
            .accessibilityLabel("Clear ink on this page")
        }
    }

    @ViewBuilder
    private func linkedNotes(_ deck: DeckInfo) -> some View {
        let notes = model.notes(on: SlideRef(deckId: deck.id, page: page))
        if !notes.isEmpty {
            VStack(alignment: .leading, spacing: 4) {
                Text("Notes on this page").font(.footnote.weight(.semibold)).foregroundStyle(.secondary)
                ForEach(notes, id: \.id) { note in
                    Text("\(TimeText.clock(note.offsetMs))  \(note.body)")
                        .font(.footnote)
                        .lineLimit(2)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.bottom, 4)
        }
    }

    private func inkGesture(_ size: CGSize) -> some Gesture {
        DragGesture(minimumDistance: 0)
            .onChanged { value in
                let point = Point(x: Float(value.location.x / size.width), y: Float(value.location.y / size.height))
                if tool == .eraser {
                    changeInk { library, session, deck, page in
                        _ = try library.eraseInk(sessionId: session, deckId: deck, page: page, at: point)
                    }
                } else {
                    drawing.append(point)
                }
            }
            .onEnded { _ in
                let points = drawing
                drawing = []
                guard tool == .pen || tool == .highlighter, !points.isEmpty else { return }
                let inkTool: InkTool = tool == .pen ? .pen : .highlighter
                let inkColor = color
                changeInk { try $0.addInkStroke(sessionId: $1, deckId: $2, page: $3, tool: inkTool, color: inkColor, points: points) }
            }
    }

    private func draw(_ points: [Point], width: Float, color: Color, in size: CGSize, context: inout GraphicsContext) {
        guard let first = points.first else { return }
        var path = Path()
        path.move(to: CGPoint(x: CGFloat(first.x) * size.width, y: CGFloat(first.y) * size.height))
        for point in points.dropFirst() {
            path.addLine(to: CGPoint(x: CGFloat(point.x) * size.width, y: CGFloat(point.y) * size.height))
        }
        if points.count == 1 {
            path.addLine(to: CGPoint(x: CGFloat(first.x) * size.width + 0.5, y: CGFloat(first.y) * size.height))
        }
        context.stroke(path, with: .color(color), style: StrokeStyle(lineWidth: CGFloat(width) * size.width, lineCap: .round, lineJoin: .round))
    }

    /// The page's size fitted into the space available.
    private func fitted(in space: CGSize) -> CGSize {
        guard let image = pageImage, image.size.width > 0, image.size.height > 0, space.width > 0, space.height > 0 else {
            return CGSize(width: max(space.width, 1), height: max(space.width * 0.75, 1))
        }
        let scale = min(space.width / image.size.width, space.height / image.size.height)
        return CGSize(width: image.size.width * scale, height: image.size.height * scale)
    }

    private func loadPage() {
        guard let deck else { return }
        model.currentSlide = SlideRef(deckId: deck.id, page: page)
        if document?.documentURL?.path != deck.pdfPath {
            document = PDFDocument(url: URL(fileURLWithPath: deck.pdfPath))
        }
        if let pdfPage = document?.page(at: Int(page) - 1) {
            let bounds = pdfPage.bounds(for: .cropBox)
            let rotated = pdfPage.rotation % 180 != 0
            let aspect = rotated ? bounds.width / max(bounds.height, 1) : bounds.height / max(bounds.width, 1)
            pageImage = pdfPage.thumbnail(of: CGSize(width: Self.renderWidth, height: Self.renderWidth * aspect), for: .cropBox)
        } else {
            pageImage = nil
        }
        reloadInk()
    }

    private func reloadInk() {
        guard let deck else { return }
        do {
            strokes = try model.library.ink(sessionId: model.sessionId, deckId: deck.id, page: page)
        } catch {
            model.problem = errorText(error)
        }
    }

    private func changeInk(_ change: (LectioLibrary, String, String, UInt32) throws -> Void) {
        guard let deck else { return }
        do {
            try change(model.library, model.sessionId, deck.id, page)
            reloadInk()
        } catch {
            model.problem = errorText(error)
        }
    }
}
