import SwiftUI

/// One lecture: the audio bar on top, then Notes, Slides, Transcript and
/// Study. The notes composer stamps each note with the current moment of
/// the recording (while recording) or of playback (while reviewing).
struct SessionView: View {
    enum Tab: String, CaseIterable, Identifiable {
        case notes = "Notes"
        case slides = "Slides"
        case transcript = "Transcript"
        case study = "Study"

        var id: String { rawValue }
    }

    @ObservedObject var app: AppModel
    @EnvironmentObject private var recorder: RecorderModel
    @EnvironmentObject private var player: PlayerModel
    @StateObject private var model: SessionModel
    @StateObject private var transcription = TranscriptionModel()
    @StateObject private var study: StudyModel
    @State private var tab: Tab = .notes
    @State private var shared: SharedFile?

    init(app: AppModel, sessionId: String) {
        self.app = app
        let changed: () -> Void = { [weak app] in app?.changed() }
        _model = StateObject(wrappedValue: SessionModel(sessionId: sessionId, library: app.library, changed: changed))
        _study = StateObject(wrappedValue: StudyModel(sessionId: sessionId, library: app.library, changed: changed))
    }

    var body: some View {
        VStack(spacing: 0) {
            AudioBar(model: model)
            Picker("Section", selection: $tab) {
                ForEach(Tab.allCases) { tab in
                    Text(tab.rawValue).tag(tab)
                }
            }
            .pickerStyle(.segmented)
            .padding(.horizontal)
            .padding(.vertical, 8)
            .accessibilityIdentifier("session.tabs")
            Divider()
            content
                .frame(maxHeight: .infinity)
        }
        .navigationTitle(model.title)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                ExportMenu(model: model) { shared = SharedFile(url: $0) }
            }
        }
        .sheet(item: $shared) { file in
            ShareSheet(items: [file.url])
        }
        .safeAreaInset(edge: .top, spacing: 0) {
            if let status = model.status {
                Text(status)
                    .font(.footnote)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 6)
                    .background(Capsule().fill(.thinMaterial))
                    .padding(.top, 4)
                    .accessibilityIdentifier("session.status")
            }
        }
        .alert("Something went wrong", isPresented: problemShown) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(model.problem ?? recorder.problem ?? player.problem ?? "")
        }
        .onAppear(perform: refresh)
        .onChange(of: app.revision) { _ in refresh() }
    }

    @ViewBuilder
    private var content: some View {
        switch tab {
        case .notes:
            NoteFeedView(model: model, offsetNow: offsetNow, playFrom: play)
        case .slides:
            SlidesView(model: model, offsetNow: offsetNow)
        case .transcript:
            TranscriptView(model: model, job: transcription, playFrom: play)
        case .study:
            StudyView(model: model, study: study, playFrom: play)
        }
    }

    /// Now, in the recording: what has been recorded so far, or where
    /// playback is.
    private func offsetNow() -> UInt64 {
        if recorder.isRecording(model.sessionId) { return recorder.elapsedMs }
        if player.isLoaded(model.sessionId) { return player.positionMs }
        return 0
    }

    private func play(from ms: UInt64) {
        guard let path = model.recordingPath else { return }
        player.play(session: model.sessionId, path: path, from: ms)
    }

    private func refresh() {
        model.reload()
        if let path = model.recordingPath {
            player.prepare(session: model.sessionId, path: path)
        }
    }

    private var problemShown: Binding<Bool> {
        Binding(
            get: { model.problem != nil || recorder.problem != nil || player.problem != nil },
            set: { shown in
                guard !shown else { return }
                model.problem = nil
                recorder.problem = nil
                player.problem = nil
            }
        )
    }
}

/// Record, or play back with speed and a scrubber.
struct AudioBar: View {
    @ObservedObject var model: SessionModel
    @EnvironmentObject private var recorder: RecorderModel
    @EnvironmentObject private var player: PlayerModel

    var body: some View {
        Group {
            if recorder.isRecording(model.sessionId) {
                recording
            } else if model.recordingPath != nil {
                playback
            } else {
                Button {
                    Task { await recorder.start(session: model.sessionId) }
                } label: {
                    Label("Record", systemImage: "record.circle")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                .tint(.red)
                .disabled(recorder.state != .idle)
                .accessibilityIdentifier("audio.record")
                .accessibilityHint("Records the lecture. Notes you add are linked to this moment.")
            }
        }
        .padding(.horizontal)
        .padding(.top, 8)
    }

    private var recording: some View {
        HStack(spacing: 12) {
            Image(systemName: recorder.state == .interrupted ? "pause.circle.fill" : "record.circle.fill")
                .foregroundStyle(.red)
                .font(.title2)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(recorder.state == .interrupted ? "Paused by a call or Siri" : "Recording")
                    .font(.subheadline.weight(.semibold))
                Text(TimeText.clock(recorder.elapsedMs) + (recorder.inputName.isEmpty ? "" : " · \(recorder.inputName)"))
                    .font(.subheadline.monospacedDigit())
                    .foregroundStyle(.secondary)
                    .accessibilityIdentifier("audio.elapsed")
            }
            .accessibilityElement(children: .combine)
            Spacer()
            if recorder.state == .interrupted {
                Button("Resume") { recorder.resume() }
                    .buttonStyle(.bordered)
            }
            Button {
                recorder.stop()
            } label: {
                Label("Stop", systemImage: "stop.fill")
            }
            .buttonStyle(.borderedProminent)
            .accessibilityIdentifier("audio.stop")
        }
    }

    private var playback: some View {
        VStack(spacing: 4) {
            HStack(spacing: 12) {
                Button {
                    if player.isPlaying && player.isLoaded(model.sessionId) {
                        player.pause()
                    } else if let path = model.recordingPath {
                        player.play(session: model.sessionId, path: path)
                    }
                } label: {
                    Image(systemName: isPlayingHere ? "pause.fill" : "play.fill")
                        .font(.title2)
                        .frame(minWidth: 44, minHeight: 44)
                }
                .accessibilityLabel(isPlayingHere ? "Pause" : "Play")
                .accessibilityIdentifier("audio.play")
                Slider(value: position, in: 0...Double(max(player.durationMs, 1)))
                    .accessibilityLabel("Position")
                    .accessibilityValue(TimeText.spoken(isLoadedHere ? player.positionMs : 0))
                Menu {
                    Picker("Speed", selection: Binding(get: { player.speedPercent }, set: { player.setSpeed($0) })) {
                        ForEach(PlayerModel.speeds, id: \.self) { speed in
                            Text(speedText(speed)).tag(speed)
                        }
                    }
                } label: {
                    Text(speedText(player.speedPercent))
                        .font(.subheadline.monospacedDigit())
                        .frame(minWidth: 44, minHeight: 44)
                }
                .accessibilityLabel("Speed, \(speedText(player.speedPercent))")
            }
            HStack {
                Text(TimeText.clock(isLoadedHere ? player.positionMs : 0))
                Spacer()
                Text(TimeText.clock(isLoadedHere ? player.durationMs : model.detail?.info.durationMs ?? 0))
            }
            .font(.caption.monospacedDigit())
            .foregroundStyle(.secondary)
            .accessibilityHidden(true)
        }
    }

    private var isLoadedHere: Bool { player.isLoaded(model.sessionId) }
    private var isPlayingHere: Bool { isLoadedHere && player.isPlaying }

    private var position: Binding<Double> {
        Binding(
            get: { isLoadedHere ? Double(player.positionMs) : 0 },
            set: { value in
                guard let path = model.recordingPath else { return }
                if !isLoadedHere { player.prepare(session: model.sessionId, path: path) }
                player.seek(to: UInt64(max(0, value)))
            }
        )
    }

    private func speedText(_ percent: UInt16) -> String {
        let value = Double(percent) / 100
        return value.formatted(.number.precision(.fractionLength(0...2))) + "×"
    }
}

/// PDF, Word or Markdown, through the share sheet.
struct ExportMenu: View {
    @ObservedObject var model: SessionModel
    let share: (URL) -> Void
    @State private var includeTranscript = false

    var body: some View {
        Menu {
            Toggle("Include the transcript", isOn: $includeTranscript)
            Button("PDF") { export(.pdf) }
            Button("Word (.docx)") { export(.docx) }
            Button("Markdown") { export(.markdown) }
        } label: {
            if model.isExporting {
                ProgressView()
            } else {
                Label("Export", systemImage: "square.and.arrow.up")
            }
        }
        .disabled(model.isExporting)
        .accessibilityIdentifier("session.export")
    }

    private func export(_ format: ExportFormat) {
        Task {
            if let url = await model.export(format, includeTranscript: includeTranscript) {
                share(url)
            }
        }
    }
}
