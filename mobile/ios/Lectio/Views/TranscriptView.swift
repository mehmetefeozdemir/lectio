import SwiftUI

/// The transcript, made on this device; each line plays from its moment.
struct TranscriptView: View {
    @ObservedObject var model: SessionModel
    @ObservedObject var job: TranscriptionModel
    let playFrom: (UInt64) -> Void
    @EnvironmentObject private var app: AppModel
    @EnvironmentObject private var store: ModelStore

    var body: some View {
        List {
            Section {
                controls
            } footer: {
                Text("Transcription runs on this device; nothing is uploaded. The first time, the model is downloaded once.")
            }
            Section("Transcript") {
                let segments = model.detail?.transcript ?? []
                if segments.isEmpty {
                    Text(model.recordingPath == nil ? "Record the lecture first." : "No transcript yet.")
                        .foregroundStyle(.secondary)
                }
                ForEach(Array(segments.enumerated()), id: \.offset) { _, segment in
                    Button { playFrom(segment.startMs) } label: {
                        HStack(alignment: .firstTextBaseline, spacing: 12) {
                            Text(TimeText.clock(segment.startMs))
                                .font(.callout.monospacedDigit())
                                .foregroundStyle(.secondary)
                            Text(segment.text)
                                .foregroundStyle(.primary)
                                .multilineTextAlignment(.leading)
                        }
                    }
                    .accessibilityLabel(segment.text)
                    .accessibilityHint("Plays from \(TimeText.spoken(segment.startMs))")
                }
            }
        }
    }

    @ViewBuilder
    private var controls: some View {
        Picker("Model", selection: $job.modelName) {
            ForEach(job.models, id: \.name) { Text($0.displayName).tag($0.name) }
        }
        .disabled(job.isRunning)
        switch job.phase {
        case let .downloading(fraction):
            progressRow("Downloading the model", fraction)
        case .loading:
            progressRow("Loading the model", nil)
        case let .transcribing(fraction):
            progressRow("Transcribing", fraction)
        case let .finished(count, seconds):
            Text(finishedText(count, seconds)).foregroundStyle(.secondary)
        case let .failed(message):
            Text(message).foregroundStyle(.red)
        case .idle:
            EmptyView()
        }
        if job.isRunning {
            Button("Stop", role: .cancel) { job.cancel() }
        } else {
            Button {
                Task {
                    await job.transcribe(session: model.sessionId, library: app.library, store: store) { model.reload() }
                }
            } label: {
                Label("Transcribe", systemImage: "text.bubble")
            }
            .disabled(model.recordingPath == nil)
        }
    }

    /// "Done: 7 segments in 12.3 s (2.1× real time)."
    private func finishedText(_ count: UInt32, _ seconds: Double) -> String {
        let taken = seconds.formatted(.number.precision(.fractionLength(1)))
        guard let audioMs = model.detail?.info.durationMs, audioMs > 0, seconds > 0 else {
            return "Done: \(count) segments in \(taken) s."
        }
        let speed = (Double(audioMs) / 1000 / seconds).formatted(.number.precision(.fractionLength(1)))
        return "Done: \(count) segments in \(taken) s (\(speed)× real time)."
    }

    private func progressRow(_ title: String, _ fraction: Double?) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title)
            if let fraction {
                ProgressView(value: fraction)
                    .accessibilityValue("\(Int(fraction * 100)) percent")
            } else {
                ProgressView()
            }
        }
    }
}

/// Outline, quiz and flashcards for the lecture.
struct StudyView: View {
    @ObservedObject var model: SessionModel
    @ObservedObject var study: StudyModel
    let playFrom: (UInt64) -> Void
    @EnvironmentObject private var store: ModelStore
    @State private var typed = ""

    var body: some View {
        List {
            if let busy = study.busy {
                Section {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(busy.text)
                        ProgressView(value: busy.progress)
                    }
                    Button("Stop", role: .cancel) { study.cancelJob() }
                }
            }
            if let message = study.message {
                Section { Text(message) }
            }
            quizSection
            cardSection
            outlineSection
        }
        .onAppear { study.loadDueCards() }
        .alert("Something went wrong", isPresented: Binding(get: { study.problem != nil }, set: { if !$0 { study.problem = nil } })) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(study.problem ?? "")
        }
    }

    @ViewBuilder
    private var quizSection: some View {
        Section("Quiz") {
            if let quiz = study.quiz, let question = quiz.current {
                Text("Question \(quiz.index + 1) of \(quiz.questions.count)")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                Text(question.prompt).font(.headline)
                if question.choices.isEmpty {
                    TextField("Your answer", text: $typed)
                        .autocorrectionDisabled()
                        .onSubmit { study.answer(typed) }
                    Button("Check") { study.answer(typed) }
                        .disabled(quiz.lastAnswerRight != nil)
                } else {
                    ForEach(question.choices, id: \.self) { choice in
                        Button(choice) { study.answer(choice) }
                            .disabled(quiz.lastAnswerRight != nil)
                    }
                }
                if let right = quiz.lastAnswerRight {
                    Label(right ? "Right" : "The answer is: \(question.answer)", systemImage: right ? "checkmark.circle.fill" : "xmark.circle.fill")
                        .foregroundStyle(right ? .green : .red)
                    if !question.explanation.isEmpty {
                        Text(question.explanation).font(.callout)
                    }
                    if let offset = question.offsetMs, model.recordingPath != nil {
                        Button { playFrom(offset) } label: {
                            Label("Play that moment (\(TimeText.clock(offset)))", systemImage: "play.circle")
                        }
                    }
                    Button("Next") {
                        typed = ""
                        study.nextQuestion()
                    }
                }
            } else {
                Button { study.startOfflineQuiz() } label: {
                    Label("Quiz from my Definition notes", systemImage: "questionmark.circle")
                }
                Button { Task { await study.writeQuiz(store: store) } } label: {
                    Label("Write questions with the study model", systemImage: "sparkles")
                }
                .disabled(study.busy != nil)
                if let last = model.detail?.quizScores.last {
                    Text("Last score: \(last.correct) of \(last.asked)").foregroundStyle(.secondary)
                }
            }
        }
    }

    @ViewBuilder
    private var cardSection: some View {
        Section("Flashcards") {
            if let card = study.currentCard {
                Button { study.showsBack.toggle() } label: {
                    VStack(alignment: .leading, spacing: 8) {
                        Text(card.front).font(.headline)
                        if study.showsBack {
                            Divider()
                            Text(card.back)
                        } else {
                            Text("Tap to show the answer").font(.footnote).foregroundStyle(.secondary)
                        }
                    }
                    .foregroundStyle(.primary)
                }
                if study.showsBack {
                    HStack {
                        gradeButton("Again", .again)
                        gradeButton("Hard", .hard)
                        gradeButton("Good", .good)
                        gradeButton("Easy", .easy)
                    }
                }
                Text("\(study.cards.count) due").font(.footnote).foregroundStyle(.secondary)
            } else {
                Text("No cards due from this lecture.").foregroundStyle(.secondary)
            }
            Button { study.makeCards() } label: {
                Label("Make cards from Definition notes", systemImage: "rectangle.stack.badge.plus")
            }
        }
    }

    @ViewBuilder
    private var outlineSection: some View {
        Section("Outline") {
            let items = model.detail?.outline ?? []
            if items.isEmpty {
                Text("The study model writes an outline of the lecture, each point linked to its moment.")
                    .foregroundStyle(.secondary)
            }
            ForEach(Array(items.enumerated()), id: \.offset) { _, item in
                Button {
                    if let offset = item.offsetMs { playFrom(offset) }
                } label: {
                    HStack {
                        Text(item.title)
                            .font(item.level <= 1 ? .headline : .body)
                            .padding(.leading, CGFloat(max(Int(item.level) - 1, 0)) * 16)
                        Spacer()
                        if let offset = item.offsetMs {
                            Text(TimeText.clock(offset)).font(.caption.monospacedDigit()).foregroundStyle(.secondary)
                        }
                    }
                    .foregroundStyle(.primary)
                }
                .disabled(item.offsetMs == nil || model.recordingPath == nil)
            }
            Button { Task { await study.writeOutline(store: store) } } label: {
                Label(items.isEmpty ? "Write an outline" : "Write it again", systemImage: "list.bullet.indent")
            }
            .disabled(study.busy != nil)
        }
    }

    private func gradeButton(_ title: String, _ grade: Grade) -> some View {
        Button(title) { study.grade(grade) }
            .buttonStyle(.bordered)
            .frame(maxWidth: .infinity)
    }
}
