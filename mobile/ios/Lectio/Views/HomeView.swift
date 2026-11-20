import SwiftUI

/// Lectures on the left, the open lecture on the right (iPad, and iPhone in
/// landscape on larger models); one column that pushes on smaller screens.
struct RootView: View {
    @ObservedObject var app: AppModel
    @StateObject private var home: HomeModel
    @State private var selection: String?

    init(app: AppModel) {
        self.app = app
        _home = StateObject(wrappedValue: HomeModel(library: app.library, changed: { [weak app] in app?.changed() }))
    }

    var body: some View {
        NavigationSplitView {
            HomeView(home: home, selection: $selection)
        } detail: {
            if let selection {
                SessionView(app: app, sessionId: selection)
                    .id(selection)
            } else {
                VStack(spacing: 8) {
                    Image(systemName: "book.pages")
                        .font(.largeTitle)
                        .foregroundStyle(.secondary)
                        .accessibilityHidden(true)
                    Text("Choose a lecture, or start a new one.")
                        .foregroundStyle(.secondary)
                }
            }
        }
        .onChange(of: app.revision) { _ in home.reload() }
    }
}

struct HomeView: View {
    @ObservedObject var home: HomeModel
    @Binding var selection: String?
    @State private var askingCourse = false
    @State private var courseName = ""
    @State private var askingCollection = false
    @State private var collectionName = ""
    @State private var creatingLecture = false
    @State private var createdLecture: String?

    var body: some View {
        List(selection: $selection) {
            if home.isSearching {
                searchResults
            } else {
                ForEach(home.sessions, id: \.id) { session in
                    SessionRow(session: session)
                        .tag(session.id)
                        .contextMenu { sessionMenu(session) }
                        .swipeActions {
                            Button(role: .destructive) { home.trash(session: session.id) } label: {
                                Label("Move to Trash", systemImage: "trash")
                            }
                        }
                }
            }
        }
        .overlay { emptyState }
        .navigationTitle(home.filterTitle)
        .searchable(text: $home.query, prompt: "Search notes, slides and transcripts")
        .toolbar {
            ToolbarItemGroup(placement: .primaryAction) {
                filterMenu
                Button { creatingLecture = true } label: {
                    Label("New lecture", systemImage: "plus")
                }
                .disabled(home.courses.isEmpty)
                .accessibilityIdentifier("home.newLecture")
            }
        }
        .alert("New course", isPresented: $askingCourse) {
            TextField("Course name", text: $courseName)
            Button("Create") {
                home.addCourse(named: courseName)
                courseName = ""
            }
            Button("Cancel", role: .cancel) { courseName = "" }
        }
        .alert("New collection", isPresented: $askingCollection) {
            TextField("Collection name", text: $collectionName)
            Button("Create") {
                home.addCollection(named: collectionName)
                collectionName = ""
            }
            Button("Cancel", role: .cancel) { collectionName = "" }
        }
        .sheet(isPresented: $creatingLecture, onDismiss: openCreatedLecture) {
            NewLectureSheet(home: home) { createdLecture = $0 }
        }
        .alert("Something went wrong", isPresented: Binding(get: { home.problem != nil }, set: { if !$0 { home.problem = nil } })) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(home.problem ?? "")
        }
        .onAppear { home.reload() }
    }

    private func openCreatedLecture() {
        guard let id = createdLecture else { return }
        createdLecture = nil
        selection = id
    }

    @ViewBuilder
    private var emptyState: some View {
        if home.courses.isEmpty {
            VStack(spacing: 12) {
                Text("Start with a course")
                    .font(.title3.weight(.semibold))
                Text("Lectures belong to a course, like \"Biology 101\".")
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                Button("New course") { askingCourse = true }
                    .buttonStyle(.borderedProminent)
                    .accessibilityIdentifier("home.newCourse")
            }
            .padding()
        } else if home.sessions.isEmpty && !home.isSearching {
            Text("No lectures here yet. Tap + to start one.")
                .foregroundStyle(.secondary)
                .padding()
        } else if home.isSearching && home.hits.isEmpty {
            Text("Nothing matches \"\(home.query)\".")
                .foregroundStyle(.secondary)
                .padding()
        }
    }

    private var searchResults: some View {
        ForEach(Array(home.hits.enumerated()), id: \.offset) { _, hit in
            VStack(alignment: .leading, spacing: 4) {
                Text(hit.sessionTitle)
                    .font(.headline)
                Text(hit.snippet)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .lineLimit(3)
                if let offset = hit.offsetMs {
                    Text(TimeText.clock(offset))
                        .font(.caption.monospacedDigit())
                        .foregroundStyle(.secondary)
                        .accessibilityLabel("at \(TimeText.spoken(offset))")
                }
            }
            .accessibilityElement(children: .combine)
            .tag(hit.sessionId)
        }
    }

    private var filterMenu: some View {
        Menu {
            Picker("Show", selection: $home.filter) {
                Text("All lectures").tag(HomeFilter.all)
                ForEach(home.courses, id: \.id) { course in
                    Text(course.name).tag(HomeFilter.course(id: course.id))
                }
                ForEach(home.collections, id: \.id) { collection in
                    Label(collection.name, systemImage: "folder").tag(HomeFilter.collection(id: collection.id))
                }
                ForEach(NoteLabel.all, id: \.self) { label in
                    Label(label.title, systemImage: label.symbol).tag(HomeFilter.label(label: label))
                }
            }
            Picker("Sort", selection: $home.sort) {
                Text("Newest first").tag(SessionSort.newest)
                Text("Oldest first").tag(SessionSort.oldest)
                Text("Title").tag(SessionSort.title)
                Text("Longest").tag(SessionSort.longest)
            }
            Divider()
            Button { askingCourse = true } label: { Label("New course", systemImage: "graduationcap") }
            Button { askingCollection = true } label: { Label("New collection", systemImage: "folder.badge.plus") }
            if case let .collection(id) = home.filter {
                Button(role: .destructive) { home.deleteCollection(id) } label: {
                    Label("Delete this collection", systemImage: "folder.badge.minus")
                }
            }
        } label: {
            Label("Show and sort", systemImage: "line.3.horizontal.decrease.circle")
        }
        .accessibilityIdentifier("home.filter")
    }

    @ViewBuilder
    private func sessionMenu(_ session: SessionInfo) -> some View {
        if !home.collections.isEmpty {
            Menu("Collections") {
                ForEach(home.collections, id: \.id) { collection in
                    let inside = collection.sessionIds.contains(session.id)
                    Button {
                        if inside {
                            home.remove(session: session.id, fromCollection: collection.id)
                        } else {
                            home.add(session: session.id, toCollection: collection.id)
                        }
                    } label: {
                        Label(collection.name, systemImage: inside ? "checkmark" : "folder")
                    }
                }
            }
        }
        Button(role: .destructive) { home.trash(session: session.id) } label: {
            Label("Move to Trash", systemImage: "trash")
        }
    }
}

struct SessionRow: View {
    let session: SessionInfo

    private var created: Date {
        Date(timeIntervalSince1970: TimeInterval(session.createdUnixMs) / 1000)
    }

    /// Icon and text side by side (a sidebar list shows `Label` as an icon only).
    private func detail(_ text: String, symbol: String, spoken: String) -> some View {
        HStack(spacing: 4) {
            Image(systemName: symbol)
            Text(text).fixedSize()
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(spoken)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(session.title)
                .font(.headline)
            Text(session.courseName + " · " + created.formatted(date: .abbreviated, time: .omitted))
                .font(.subheadline)
                .foregroundStyle(.secondary)
            HStack(spacing: 12) {
                if let duration = session.durationMs {
                    detail(TimeText.clock(duration), symbol: "waveform", spoken: "recorded \(TimeText.spoken(duration))")
                }
                detail("\(session.noteCount)", symbol: "note.text", spoken: session.noteCount == 1 ? "1 note" : "\(session.noteCount) notes")
            }
            .font(.subheadline.monospacedDigit())
            .foregroundStyle(.secondary)
        }
        .accessibilityElement(children: .combine)
    }
}

/// Picks the course and title for a new lecture.
struct NewLectureSheet: View {
    @ObservedObject var home: HomeModel
    let created: (String) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var courseId = ""
    @State private var title = HomeModel.suggestedTitle()

    var body: some View {
        NavigationStack {
            Form {
                Picker("Course", selection: $courseId) {
                    ForEach(home.courses, id: \.id) { course in
                        Text(course.name).tag(course.id)
                    }
                }
                TextField("Title", text: $title)
                    .accessibilityIdentifier("newLecture.title")
            }
            .navigationTitle("New lecture")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Create") {
                        if let id = home.addSession(courseId: courseId, title: title) {
                            created(id)
                            dismiss()
                        }
                    }
                    .disabled(courseId.isEmpty || title.trimmingCharacters(in: .whitespaces).isEmpty)
                    .accessibilityIdentifier("newLecture.create")
                }
            }
            .onAppear {
                if case let .course(id) = home.filter {
                    courseId = id
                } else {
                    courseId = home.courses.first?.id ?? ""
                }
            }
        }
    }
}
