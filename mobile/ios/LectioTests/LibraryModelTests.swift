import XCTest
@testable import Lectio

/// A library in a fresh temporary folder.
func makeLibrary() throws -> LectioLibrary {
    let folder = FileManager.default.temporaryDirectory.appendingPathComponent("LectioTests-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
    return try LectioLibrary.open(dataDir: folder.path)
}

@MainActor
final class HomeModelTests: XCTestCase {
    func testCreatingACourseAndALectureShowsThemOnHome() throws {
        // Arrange
        let home = HomeModel(library: try makeLibrary())
        // Act
        let course = try XCTUnwrap(home.addCourse(named: "  Biology  "))
        let session = try XCTUnwrap(home.addSession(courseId: course, title: "Cells"))
        // Assert
        XCTAssertEqual(home.courses.map(\.name), ["Biology"])
        XCTAssertEqual(home.sessions.map(\.id), [session])
        XCTAssertEqual(home.sessions.first?.courseName, "Biology")
        XCTAssertNil(home.problem)
    }

    func testAnEmptyCourseNameIsRefusedWithAMessage() throws {
        let home = HomeModel(library: try makeLibrary())
        XCTAssertNil(home.addCourse(named: "   "))
        XCTAssertNotNil(home.problem)
    }

    func testFilteringByCourseAndCollection() throws {
        let home = HomeModel(library: try makeLibrary())
        let biology = try XCTUnwrap(home.addCourse(named: "Biology"))
        let history = try XCTUnwrap(home.addCourse(named: "History"))
        let cells = try XCTUnwrap(home.addSession(courseId: biology, title: "Cells"))
        _ = home.addSession(courseId: history, title: "Rome")
        let exam = try XCTUnwrap(home.addCollection(named: "Exam"))
        home.add(session: cells, toCollection: exam)

        home.filter = .course(id: history)
        XCTAssertEqual(home.sessions.map(\.title), ["Rome"])
        XCTAssertEqual(home.filterTitle, "History")

        home.filter = .collection(id: exam)
        XCTAssertEqual(home.sessions.map(\.title), ["Cells"])
        XCTAssertEqual(home.collections(containing: cells).map(\.name), ["Exam"])
    }

    func testADeletedCollectionFilterFallsBackToAllLectures() throws {
        let home = HomeModel(library: try makeLibrary())
        let course = try XCTUnwrap(home.addCourse(named: "Biology"))
        _ = home.addSession(courseId: course, title: "Cells")
        let exam = try XCTUnwrap(home.addCollection(named: "Exam"))
        home.filter = .collection(id: exam)
        home.deleteCollection(exam)
        XCTAssertEqual(home.filter, .all)
        XCTAssertEqual(home.sessions.count, 1)
    }

    func testSearchFindsANoteAndTrashHidesTheLecture() throws {
        let library = try makeLibrary()
        let home = HomeModel(library: library)
        let course = try XCTUnwrap(home.addCourse(named: "Biology"))
        let session = try XCTUnwrap(home.addSession(courseId: course, title: "Cells"))
        _ = try library.addNote(sessionId: session, offsetMs: 4_000, text: "Mitochondria make ATP", labels: [])

        home.query = "mitochondria"
        XCTAssertEqual(home.hits.first?.sessionId, session)
        XCTAssertEqual(home.hits.first?.offsetMs, 4_000)

        home.query = ""
        home.trash(session: session)
        XCTAssertTrue(home.sessions.isEmpty)
    }

    func testChangesTellTheApp() throws {
        var changes = 0
        let home = HomeModel(library: try makeLibrary(), changed: { changes += 1 })
        _ = home.addCourse(named: "Biology")
        XCTAssertEqual(changes, 1)
    }
}

@MainActor
final class SessionModelTests: XCTestCase {
    private func makeSession() throws -> SessionModel {
        let library = try makeLibrary()
        let course = try library.addCourse(name: "Biology")
        let session = try library.addSession(courseId: course, title: "Cells")
        let model = SessionModel(sessionId: session, library: library)
        model.reload()
        return model
    }

    func testANoteKeepsItsMomentAndLabelsAndClearsTheDraft() throws {
        let model = try makeSession()
        model.draft = "  ATP is the energy currency  "
        model.toggleDraftLabel(.important)
        model.toggleDraftLabel(.review)
        model.toggleDraftLabel(.review)

        XCTAssertTrue(model.addNote(at: 12_345))

        let note = try XCTUnwrap(model.notes.first)
        XCTAssertEqual(note.body, "ATP is the energy currency")
        XCTAssertEqual(note.offsetMs, 12_345)
        XCTAssertEqual(note.labels, [.important])
        XCTAssertEqual(model.draft, "")
        XCTAssertEqual(model.draftLabels, [])
        XCTAssertEqual(model.status, "Note added at 0:12")
    }

    func testANoteIsStampedWhenTypingStartedNotWhenItWasAdded() throws {
        let model = try makeSession()
        model.draft = "Mito"
        model.draftEdited(at: 2_000)
        model.draft = "Mitochondria make ATP"
        model.draftEdited(at: 9_000)

        model.addNote(at: 11_000)

        XCTAssertEqual(model.notes.first?.offsetMs, 2_000)
    }

    func testAnEmptyDraftAddsNothing() throws {
        let model = try makeSession()
        model.draft = "   "
        XCTAssertFalse(model.addNote(at: 0))
        XCTAssertTrue(model.notes.isEmpty)
    }

    func testLabelsToggleEditAndDelete() throws {
        let model = try makeSession()
        model.draft = "Osmosis"
        model.addNote(at: 1_000)
        var note = try XCTUnwrap(model.notes.first)

        model.toggle(.definition, on: note)
        note = try XCTUnwrap(model.notes.first)
        XCTAssertEqual(note.labels, [.definition])

        model.edit(note, text: "Osmosis: water crossing a membrane")
        XCTAssertEqual(model.notes.first?.body, "Osmosis: water crossing a membrane")

        model.delete(try XCTUnwrap(model.notes.first))
        XCTAssertTrue(model.notes.isEmpty)
    }

    func testFormulaNotesAreCheckedByTheCore() throws {
        let model = try makeSession()
        XCTAssertTrue(model.addFormula("E = mc^2", at: 2_000))
        XCTAssertEqual(model.notes.first?.kind, .formula)
        XCTAssertNil(checkFormula(latex: "\\frac{a}{b}"))
        XCTAssertNotNil(checkFormula(latex: "\\frac{a}{"))
        let png = try renderFormulaPng(latex: "\\sqrt{x}", sizePx: 40, dark: true)
        XCTAssertEqual(Array(png.prefix(4)), [0x89, 0x50, 0x4E, 0x47])
    }

    func testAPhotoBecomesAPhotoNote() throws {
        let model = try makeSession()
        let image = UIGraphicsImageRenderer(size: CGSize(width: 40, height: 30)).image { context in
            UIColor.systemTeal.setFill()
            context.fill(CGRect(x: 0, y: 0, width: 40, height: 30))
        }
        XCTAssertTrue(model.addPhoto(image, caption: "Board", at: 5_000))
        let photo = try XCTUnwrap(model.notes.first?.photo)
        XCTAssertTrue(FileManager.default.fileExists(atPath: photo.path))
        XCTAssertGreaterThan(photo.width, 0)
    }

    func testExportWritesAFileForTheShareSheet() async throws {
        let model = try makeSession()
        model.draft = "Mitochondria make ATP"
        model.addNote(at: 3_000)

        let markdown = await model.export(.markdown, includeTranscript: false)
        let pdf = await model.export(.pdf, includeTranscript: false)

        let text = try String(contentsOf: try XCTUnwrap(markdown), encoding: .utf8)
        XCTAssertTrue(text.contains("Mitochondria make ATP"))
        XCTAssertEqual(markdown?.lastPathComponent, "Cells.md")
        let bytes = try Data(contentsOf: try XCTUnwrap(pdf))
        XCTAssertEqual(Array(bytes.prefix(4)), Array("%PDF".utf8))
        XCTAssertTrue(model.status?.hasPrefix("Exported Cells.pdf") ?? false)
        XCTAssertFalse(model.isExporting)
    }
}

@MainActor
final class StudyModelTests: XCTestCase {
    func testTheOfflineQuizComesFromDefinitionNotesAndSavesTheScore() throws {
        let library = try makeLibrary()
        let session = try library.addSession(courseId: try library.addCourse(name: "Biology"), title: "Cells")
        _ = try library.addNote(sessionId: session, offsetMs: 1_000, text: "Osmosis: water moving across a membrane", labels: [.definition])
        let study = StudyModel(sessionId: session, library: library)

        study.startOfflineQuiz()
        let question = try XCTUnwrap(study.quiz?.current)
        study.answer(question.answer.uppercased())
        XCTAssertEqual(study.quiz?.lastAnswerRight, true)
        study.nextQuestion()

        XCTAssertNil(study.quiz)
        XCTAssertEqual(study.message, "Score: 1 of 1")
        XCTAssertEqual(try library.session(sessionId: session).quizScores.last?.correct, 1)
    }

    func testWithoutDefinitionsTheQuizExplainsWhatToDo() throws {
        let library = try makeLibrary()
        let session = try library.addSession(courseId: try library.addCourse(name: "Biology"), title: "Cells")
        let study = StudyModel(sessionId: session, library: library)
        study.startOfflineQuiz()
        XCTAssertNil(study.quiz)
        XCTAssertTrue(study.message?.contains("Definition") ?? false)
    }

    func testFlashcardsAreMadeAndScheduledAfterGrading() throws {
        let library = try makeLibrary()
        let session = try library.addSession(courseId: try library.addCourse(name: "Biology"), title: "Cells")
        _ = try library.addNote(sessionId: session, offsetMs: 1_000, text: "ATP: the cell's energy currency", labels: [.definition])
        let now: UInt64 = 1_800_000_000_000
        let study = StudyModel(sessionId: session, library: library, now: { now })

        study.makeCards()
        XCTAssertEqual(study.cards.count, 1)
        XCTAssertEqual(study.currentCard?.front, "ATP")

        study.grade(.good)
        XCTAssertTrue(study.cards.isEmpty, "a card known well isn't due again at once")
    }
}
