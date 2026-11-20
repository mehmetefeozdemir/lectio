import Foundation

/// Study tools for one lecture, with the desktop's rules from the core:
/// a quiz from Definition notes (no AI needed), flashcards scheduled with
/// SM-2, and the study model's outline and questions (every item cites a
/// moment of the lecture).
@MainActor
final class StudyModel: ObservableObject {
    static let quizLength: UInt32 = 10
    static let aiQuizLength: UInt32 = 6
    /// The smaller study model: fits in a phone's memory.
    static let studyModelName = "small"

    struct Quiz: Equatable {
        var questions: [QuizQuestionInfo]
        var index = 0
        var correct = 0
        /// The verdict on the current question, once answered.
        var lastAnswerRight: Bool?

        var current: QuizQuestionInfo? {
            questions.indices.contains(index) ? questions[index] : nil
        }

        var isOver: Bool { index >= questions.count }
    }

    @Published private(set) var quiz: Quiz?
    @Published private(set) var cards: [CardInfo] = []
    @Published var showsBack = false
    /// What a running AI job is doing, with progress 0...1.
    @Published private(set) var busy: (text: String, progress: Double)?
    @Published var problem: String?
    @Published private(set) var message: String?

    let sessionId: String
    private let library: LectioLibrary
    private let changed: () -> Void
    private let now: () -> UInt64
    private var job: JobProgress?

    init(sessionId: String, library: LectioLibrary, changed: @escaping () -> Void = {}, now: @escaping () -> UInt64 = nowUnixMs) {
        self.sessionId = sessionId
        self.library = library
        self.changed = changed
        self.now = now
    }

    var currentCard: CardInfo? { cards.first }

    // MARK: Quiz

    /// Fill-in-the-blank questions from this lecture's Definition notes.
    func startOfflineQuiz() {
        do {
            let questions = try library.offlineQuiz(sessionIds: [sessionId], count: Self.quizLength)
            guard !questions.isEmpty else {
                message = "Label a few notes as Definition (\"Term: meaning\") to get a quiz without AI."
                return
            }
            quiz = Quiz(questions: questions)
        } catch {
            problem = errorText(error)
        }
    }

    /// Checks a typed or chosen answer (the desktop's rule: every word of
    /// the answer, ignoring case, accents and punctuation).
    func answer(_ given: String) {
        guard var current = quiz, let question = current.current, current.lastAnswerRight == nil else { return }
        let right = question.choices.isEmpty ? gradeAnswer(given: given, expected: question.answer) : given == question.answer
        current.lastAnswerRight = right
        if right { current.correct += 1 }
        quiz = current
    }

    func nextQuestion() {
        guard var current = quiz else { return }
        current.index += 1
        current.lastAnswerRight = nil
        quiz = current
        if current.isOver { finishQuiz() }
    }

    /// Saves the score with the lecture (as the desktop does).
    func finishQuiz() {
        guard let current = quiz else { return }
        let asked = UInt32(min(current.index, current.questions.count))
        if asked > 0 {
            do {
                try library.recordQuizAttempt(sessionId: sessionId, asked: asked, correct: UInt32(current.correct))
                changed()
            } catch {
                problem = errorText(error)
            }
        }
        message = "Score: \(current.correct) of \(asked)"
        quiz = nil
    }

    // MARK: Flashcards

    func makeCards() {
        do {
            let made = try library.makeDefinitionCards(sessionId: sessionId, nowUnixMs: now())
            message = made == 0 ? "No new Definition notes to make cards from." : "Made \(made) card\(made == 1 ? "" : "s")."
            loadDueCards()
            changed()
        } catch {
            problem = errorText(error)
        }
    }

    /// Cards due now, from this lecture.
    func loadDueCards() {
        do {
            cards = try library.dueCards(nowUnixMs: now()).filter { $0.sessionId == sessionId }
            showsBack = false
        } catch {
            problem = errorText(error)
        }
    }

    func grade(_ grade: Grade) {
        guard let card = currentCard else { return }
        do {
            try library.gradeCard(cardId: card.id, grade: grade, nowUnixMs: now())
            loadDueCards()
            changed()
        } catch {
            problem = errorText(error)
        }
    }

    // MARK: Study model

    var studyModel: ModelInfo? {
        studyModels().first { $0.name == Self.studyModelName }
    }

    func writeOutline(store: ModelStore) async {
        guard let engine = await loadEngine(store: store) else { return }
        let progress = startJob("Writing the outline")
        let library = self.library
        let session = sessionId
        do {
            _ = try await Task.detached(priority: .userInitiated) {
                try engine.writeOutline(library: library, sessionId: session, listener: progress)
            }.value
            changed()
        } catch {
            problem = errorText(error)
        }
        endJob()
    }

    func writeQuiz(store: ModelStore) async {
        guard let engine = await loadEngine(store: store) else { return }
        let progress = startJob("Writing questions")
        let library = self.library
        let session = sessionId
        let count = Self.aiQuizLength
        do {
            let questions = try await Task.detached(priority: .userInitiated) {
                try engine.writeQuiz(library: library, sessionId: session, count: count, listener: progress)
            }.value
            quiz = Quiz(questions: questions)
        } catch {
            problem = errorText(error)
        }
        endJob()
    }

    func cancelJob() {
        job?.cancel()
    }

    private func loadEngine(store: ModelStore) async -> StudyEngine? {
        guard busy == nil, let model = studyModel else { return nil }
        do {
            if !store.isDownloaded(model) {
                busy = ("Downloading the study model", 0)
                try await store.download(model) { [weak self] fraction in
                    Task { @MainActor in self?.busy = ("Downloading the study model", fraction) }
                }
            }
            busy = ("Loading the study model", 0)
            let engine = try await store.studyEngine(model)
            busy = nil
            return engine
        } catch {
            busy = nil
            problem = errorText(error)
            return nil
        }
    }

    private func startJob(_ text: String) -> JobProgress {
        busy = (text, 0)
        let progress = JobProgress { [weak self] fraction in
            Task { @MainActor in self?.busy = (text, fraction) }
        }
        job = progress
        return progress
    }

    private func endJob() {
        job = nil
        busy = nil
    }
}
