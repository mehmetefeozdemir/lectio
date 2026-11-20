import SwiftUI
import UIKit

/// The message to show for an error. Errors from the Rust core already
/// carry a sentence written for the user.
func errorText(_ error: Error) -> String {
    if let lectio = error as? LectioError, case let .Failed(message) = lectio {
        return message
    }
    return error.localizedDescription
}

/// Milliseconds since 1970, the clock the core's cards and quizzes use.
func nowUnixMs() -> UInt64 {
    UInt64(Date().timeIntervalSince1970 * 1000)
}

enum TimeText {
    /// "0:07", "12:30", "1:02:03".
    static func clock(_ ms: UInt64) -> String {
        let seconds = ms / 1000
        let hours = seconds / 3600
        let minutes = (seconds % 3600) / 60
        let rest = seconds % 60
        let mm = minutes < 10 && hours > 0 ? "0\(minutes)" : "\(minutes)"
        let ss = rest < 10 ? "0\(rest)" : "\(rest)"
        return hours > 0 ? "\(hours):\(mm):\(ss)" : "\(mm):\(ss)"
    }

    /// For VoiceOver: "1 minute, 5 seconds".
    static func spoken(_ ms: UInt64) -> String {
        let formatter = DateComponentsFormatter()
        formatter.unitsStyle = .full
        formatter.allowedUnits = [.hour, .minute, .second]
        formatter.zeroFormattingBehavior = .dropAll
        let text = formatter.string(from: TimeInterval(ms / 1000)) ?? ""
        return text.isEmpty ? "0 seconds" : text
    }
}

extension NoteLabel {
    /// In the order the desktop shows them.
    static let all: [NoteLabel] = [.important, .review, .task, .definition, .heading]

    var title: String {
        switch self {
        case .important: return "Important"
        case .review: return "Review"
        case .task: return "Task"
        case .definition: return "Definition"
        case .heading: return "Heading"
        }
    }

    var symbol: String {
        switch self {
        case .important: return "star.fill"
        case .review: return "arrow.counterclockwise"
        case .task: return "checkmark.circle"
        case .definition: return "book.closed"
        case .heading: return "textformat.size"
        }
    }

    var tint: Color {
        switch self {
        case .important: return .orange
        case .review: return .purple
        case .task: return .green
        case .definition: return .teal
        case .heading: return .blue
        }
    }
}

extension InkColor {
    static let all: [InkColor] = [.black, .red, .blue, .green, .yellow]

    /// The same sRGB as the desktop and the exports (from the core).
    var color: Color {
        let rgb = [UInt8](inkRgb(color: self))
        guard rgb.count == 3 else { return .primary }
        return Color(red: Double(rgb[0]) / 255, green: Double(rgb[1]) / 255, blue: Double(rgb[2]) / 255)
    }

    var title: String {
        switch self {
        case .black: return "Black"
        case .red: return "Red"
        case .blue: return "Blue"
        case .green: return "Green"
        case .yellow: return "Yellow"
        }
    }
}

/// A label chip, as in the feed and the composer.
struct LabelChip: View {
    let label: NoteLabel
    var selected = true

    var body: some View {
        Label(label.title, systemImage: label.symbol)
            .font(.caption.weight(.semibold))
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .foregroundStyle(selected ? label.tint : .secondary)
            .background(
                Capsule().fill(selected ? label.tint.opacity(0.15) : Color(uiColor: .secondarySystemFill))
            )
    }
}

/// The system share sheet (AirDrop, Files, Mail, other apps).
struct ShareSheet: UIViewControllerRepresentable {
    let items: [Any]

    func makeUIViewController(context: Context) -> UIActivityViewController {
        UIActivityViewController(activityItems: items, applicationActivities: nil)
    }

    func updateUIViewController(_ controller: UIActivityViewController, context: Context) {}
}

/// A file to share; `Identifiable` so a sheet can present it.
struct SharedFile: Identifiable {
    let url: URL
    var id: String { url.path }
}

/// Tells VoiceOver users about something that happened off-screen.
func announce(_ text: String) {
    UIAccessibility.post(notification: .announcement, argument: text)
}
