import Foundation

/// The home screen: courses, collections and lectures, filtered and sorted
/// by the core (the desktop's rules), and search across everything.
@MainActor
final class HomeModel: ObservableObject {
    @Published var filter: HomeFilter = .all {
        didSet { reload() }
    }
    @Published var sort: SessionSort = .newest {
        didSet { reload() }
    }
    @Published var query = "" {
        didSet { search() }
    }
    @Published private(set) var courses: [CourseInfo] = []
    @Published private(set) var collections: [CollectionInfo] = []
    @Published private(set) var sessions: [SessionInfo] = []
    @Published private(set) var hits: [SearchHitInfo] = []
    @Published var problem: String?

    private let library: LectioLibrary
    private let changed: () -> Void

    init(library: LectioLibrary, changed: @escaping () -> Void = {}) {
        self.library = library
        self.changed = changed
    }

    var isSearching: Bool {
        !query.trimmingCharacters(in: .whitespaces).isEmpty
    }

    /// The heading for the current filter.
    var filterTitle: String {
        switch filter {
        case .all:
            return "All lectures"
        case let .course(id):
            return courses.first { $0.id == id }?.name ?? "Course"
        case let .collection(id):
            return collections.first { $0.id == id }?.name ?? "Collection"
        case let .label(label):
            return label.title
        }
    }

    func reload() {
        do {
            courses = try library.courses()
            collections = try library.collections()
            if !filterStillExists() {
                filter = .all
                return
            }
            sessions = try library.sessions(filter: filter, sort: sort)
        } catch {
            problem = errorText(error)
        }
        if isSearching { search() }
    }

    func search() {
        guard isSearching else {
            hits = []
            return
        }
        do {
            hits = try library.search(query: query)
        } catch {
            problem = errorText(error)
        }
    }

    /// Returns the new course's id.
    @discardableResult
    func addCourse(named name: String) -> String? {
        perform { try library.addCourse(name: name.trimmingCharacters(in: .whitespacesAndNewlines)) }
    }

    /// Returns the new lecture's id.
    @discardableResult
    func addSession(courseId: String, title: String) -> String? {
        perform { try library.addSession(courseId: courseId, title: title.trimmingCharacters(in: .whitespacesAndNewlines)) }
    }

    @discardableResult
    func addCollection(named name: String) -> String? {
        perform { try library.addCollection(name: name.trimmingCharacters(in: .whitespacesAndNewlines)) }
    }

    func deleteCollection(_ id: String) {
        perform { try library.deleteCollection(collectionId: id) }
    }

    func add(session: String, toCollection id: String) {
        perform { try library.addToCollection(collectionId: id, sessionId: session) }
    }

    func remove(session: String, fromCollection id: String) {
        perform { try library.removeFromCollection(collectionId: id, sessionId: session) }
    }

    /// Moves a lecture to the trash (the desktop can restore it).
    func trash(session: String) {
        perform { try library.trashSession(sessionId: session) }
    }

    func collections(containing session: String) -> [CollectionInfo] {
        collections.filter { $0.sessionIds.contains(session) }
    }

    /// A title for a new lecture: "Lecture, 20 Nov".
    static func suggestedTitle(on date: Date = Date()) -> String {
        "Lecture, " + date.formatted(.dateTime.day().month(.abbreviated))
    }

    private func filterStillExists() -> Bool {
        switch filter {
        case .all, .label: return true
        case let .course(id): return courses.contains { $0.id == id }
        case let .collection(id): return collections.contains { $0.id == id }
        }
    }

    @discardableResult
    private func perform<T>(_ body: () throws -> T) -> T? {
        do {
            let result = try body()
            reload()
            changed()
            return result
        } catch {
            problem = errorText(error)
            return nil
        }
    }
}
