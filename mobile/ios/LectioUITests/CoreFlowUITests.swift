import XCTest

/// The core flow, as a student would do it: create a course and a lecture,
/// record about 3 seconds while writing a note, play from the note, export
/// a PDF. The app starts on an empty library with a test tone instead of
/// the microphone (CI Simulators have none). Screenshots are kept as test
/// attachments; CI exports them as artifacts.
final class CoreFlowUITests: XCTestCase {
    private static let wait: TimeInterval = 10
    private static let recordingSeconds: TimeInterval = 3.5

    override func setUp() {
        continueAfterFailure = false
    }

    func testCreateCourseRecordNotePlayAndExport() throws {
        let app = XCUIApplication()
        app.launchArguments += ["-LectioFreshStart", "YES", "-LectioTestAudio", "YES"]
        app.launch()
        screenshot(app, "1-empty-library")

        // A course.
        app.buttons["home.newCourse"].tap()
        let courseAlert = app.alerts["New course"]
        XCTAssertTrue(courseAlert.waitForExistence(timeout: Self.wait))
        let courseName = courseAlert.textFields.firstMatch
        courseName.tap()
        courseName.typeText("Biology")
        courseAlert.buttons["Create"].tap()

        // A lecture in it.
        let newLecture = app.buttons["home.newLecture"]
        XCTAssertTrue(newLecture.waitForExistence(timeout: Self.wait))
        newLecture.tap()
        let title = app.textFields["newLecture.title"]
        XCTAssertTrue(title.waitForExistence(timeout: Self.wait))
        // Tap at the end of the suggested title, then delete it.
        title.coordinate(withNormalizedOffset: CGVector(dx: 0.97, dy: 0.5)).tap()
        let suggested = (title.value as? String) ?? ""
        title.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: suggested.count) + "Cells")
        app.buttons["newLecture.create"].tap()

        // Record, writing a note while it runs (stamped when typing starts).
        let record = app.buttons["audio.record"]
        XCTAssertTrue(record.waitForExistence(timeout: Self.wait))
        record.tap()
        let stop = app.buttons["audio.stop"]
        XCTAssertTrue(stop.waitForExistence(timeout: Self.wait))
        let started = Date()
        let composer = app.descendants(matching: .any)["composer.text"]
        composer.tap()
        composer.typeText("Mitochondria make ATP")
        app.buttons["composer.add"].tap()
        XCTAssertTrue(app.staticTexts["Mitochondria make ATP"].waitForExistence(timeout: Self.wait))
        let left = Self.recordingSeconds - Date().timeIntervalSince(started)
        if left > 0 { Thread.sleep(forTimeInterval: left) }
        screenshot(app, "2-recording-with-a-note")
        stop.tap()

        // Play from the note.
        let play = app.buttons["audio.play"]
        XCTAssertTrue(play.waitForExistence(timeout: Self.wait))
        app.buttons["note.play"].firstMatch.tap()
        let playing = expectation(for: NSPredicate(format: "label == 'Pause'"), evaluatedWith: play)
        wait(for: [playing], timeout: Self.wait)
        screenshot(app, "3-playing-from-the-note")

        // Export a PDF through the share sheet.
        app.buttons["session.export"].tap()
        let pdf = app.buttons["PDF"]
        XCTAssertTrue(pdf.waitForExistence(timeout: Self.wait))
        pdf.tap()
        let status = app.staticTexts["session.status"]
        let exported = expectation(for: NSPredicate(format: "label BEGINSWITH 'Exported Cells.pdf'"), evaluatedWith: status)
        wait(for: [exported], timeout: 30)
        Thread.sleep(forTimeInterval: 1)
        screenshot(app, "4-share-sheet")
    }

    private func screenshot(_ app: XCUIApplication, _ name: String) {
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}
