import XCTest

/// The App Store screenshots (ADR-039): the sample lecture on Home, its notes
/// while playing from a note, the slides with ink, the transcript and a quiz.
/// Run by tools/store/screenshots.sh on the 6.9" iPhone and 13" iPad
/// Simulators (light mode, a clean status bar), which exports the attachments
/// by name. Skipped in ordinary test runs.
final class StoreScreenshotUITests: XCTestCase {
    private static let wait: TimeInterval = 15

    override func setUp() {
        continueAfterFailure = false
    }

    func testStoreScreenshots() throws {
        guard ProcessInfo.processInfo.environment["LECTIO_STORE_SCREENSHOTS"] == "1" else {
            throw XCTSkip("Run by tools/store/screenshots.sh")
        }
        let app = XCUIApplication()
        app.launchArguments += ["-LectioFreshStart", "YES", "-LectioTestAudio", "YES", "-LectioSampleLibrary", "YES"]
        app.launch()
        let isPad = UIDevice.current.userInterfaceIdiom == .pad

        let lecture = app.descendants(matching: .any).matching(NSPredicate(format: "label BEGINSWITH 'Graph traversal'")).firstMatch
        XCTAssertTrue(lecture.waitForExistence(timeout: Self.wait))
        if !isPad { shot("01-home") }

        // The lecture, playing from the note at 17:05, then paused there.
        lecture.tap()
        let play = app.buttons["audio.play"]
        XCTAssertTrue(play.waitForExistence(timeout: Self.wait))
        app.buttons["Play from 17 minutes, 5 seconds"].firstMatch.tap()
        wait(for: [expectation(for: NSPredicate(format: "label == 'Pause'"), evaluatedWith: play)], timeout: Self.wait)
        play.tap()
        shot(isPad ? "01-lecture" : "02-notes")

        app.buttons["Slides"].tap()
        let next = app.buttons["Next page"]
        XCTAssertTrue(next.waitForExistence(timeout: Self.wait))
        next.tap()
        shot("03-slides")

        app.buttons["Transcript"].tap()
        XCTAssertTrue(app.staticTexts["So let's look at breadth-first search."].waitForExistence(timeout: Self.wait))
        shot("04-transcript")

        app.buttons["Study"].tap()
        app.buttons["Quiz from my Definition notes"].tap()
        XCTAssertTrue(app.staticTexts["Question 1 of 3"].waitForExistence(timeout: Self.wait))
        shot("05-quiz")
    }

    /// The whole screen, status bar included (the script overrides it).
    private func shot(_ name: String) {
        Thread.sleep(forTimeInterval: 1)
        let attachment = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}
