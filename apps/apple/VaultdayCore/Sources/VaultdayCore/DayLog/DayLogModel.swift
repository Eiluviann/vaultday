import Foundation
import Observation

/// One line of a day log, as the Day log tab and the peek show it.
public struct DayLogLine: Identifiable, Hashable, Sendable {
	public let id: String
	public let time: String
	/// `session`, `done`, `decision`, `blocker`, `status`, `note`, or another word as written.
	public let kind: String?
	public let text: String
	public let story: String?
	public let task: String?
	/// The model that wrote the line; `nil` for a fact the app or the user wrote.
	public let agent: String?
}

/// How a log line's text reads on screen. The writer puts the kind word in
/// front of the text (`decision · keep it dev only`) so the plain note reads
/// well in Obsidian; the tab already shows the kind, so that word is dropped.
/// Another verb (`Started · …`) is part of what the line says and stays.
enum DayLogText {
	static func display(_ text: String, kind: String?) -> String {
		guard let kind, let separator = text.range(of: " · "),
		      text[..<separator.lowerBound].caseInsensitiveCompare(kind) == .orderedSame
		else { return text }
		return String(text[separator.upperBound...])
	}
}

/// The kinds the tab's filter chips and the append field know.
public enum DayLogKind: String, CaseIterable, Sendable {
	case session, done, decision, blocker, status, note

	public var title: String { rawValue.capitalizedFirst }
}

/// The Day log tab: a day's log lines from the core, a kind filter, and the
/// append field that writes a line through the core's day-log writer.
@MainActor
@Observable
public final class DayLogModel {
	/// The day shown, `YYYY-MM-DD`.
	public private(set) var date = ""
	/// The log note, relative to the workspace root.
	public private(set) var path = ""
	public private(set) var lines: [DayLogLine] = []
	/// The chip that filters the list; `nil` shows every line.
	public var kindFilter: DayLogKind?
	/// The text in the append field.
	public var draft = ""
	/// What the last append or load said when it failed.
	public private(set) var errorMessage: String?
	/// The line the last append wrote, as it stands in the note.
	public private(set) var lastWrittenLine: String?

	/// The story and task the next line is about: the selected task.
	public var contextStory: String?
	public var contextTask: String?

	private var vault: WorkspaceHandle?
	private let now: () -> Date

	public init(now: @escaping () -> Date = Date.init) {
		self.now = now
	}

	/// The lines the chip lets through.
	public var visibleLines: [DayLogLine] {
		guard let kindFilter else { return lines }
		return lines.filter { $0.kind == kindFilter.rawValue }
	}

	/// "Log/2026-10-15.md · 9 lines".
	public var caption: String {
		path.isEmpty ? "" : "\(path) · \(lines.count) \(lines.count == 1 ? "line" : "lines")"
	}

	/// The kind a typed line would get, for the hint under the field.
	public var draftKind: String {
		parseLogLineShorthand(text: draft).kind
	}

	/// Points the model at the open workspace and reads the day shown (today at first).
	public func load(from workspace: WorkspaceModel, date: String? = nil) {
		vault = workspace.vault
		reload(date: date)
	}

	/// Opens the workspace at `path` without a grant (tests, and hosts outside the sandbox).
	public func load(workspacePath path: String, date: String? = nil) {
		vault = try? openWorkspace(path: path)
		reload(date: date)
	}

	/// Reads the day's log again.
	public func reload(date: String? = nil) {
		let day = date ?? (self.date.isEmpty ? DayText.string(now()) : self.date)
		self.date = day
		guard let vault else {
			lines = []
			path = ""
			return
		}
		do {
			let log = try dayLog(workspace: vault, date: day)
			path = log.relativePath
			lines = log.lines.enumerated().map { index, line in
				DayLogLine(
					id: "\(day)#\(index)",
					time: line.time,
					kind: line.kind,
					text: DayLogText.display(line.text, kind: line.kind),
					story: line.story,
					task: line.task,
					agent: line.agent,
				)
			}
			errorMessage = nil
		} catch {
			lines = []
			errorMessage = coreErrorMessage(error)
		}
	}

	/// Appends the draft as a line of today's log: a leading `decision:`,
	/// `blocked:` or `note:` sets the kind, anything else is a note; the story
	/// and task of the selected task are added. `true` when the line was written.
	@discardableResult
	public func append() -> Bool {
		guard let vault else { return false }
		let shorthand = parseLogLineShorthand(text: draft)
		let today = DayText.string(now())
		do {
			let change = try appendLog(workspace: vault, input: LogInput(
				date: today,
				time: Self.timeFormatter.string(from: now()),
				kind: shorthand.kind,
				text: shorthand.text,
				story: contextStory,
				task: contextTask,
				agent: nil,
			))
			lastWrittenLine = change.newLine
			draft = ""
			errorMessage = nil
			// The write is to today's log; show it.
			reload(date: today)
			return true
		} catch {
			errorMessage = coreErrorMessage(error)
			return false
		}
	}

	/// Shows the day `offset` days from the one shown.
	public func showDay(offset: Int) {
		guard let shown = DayText.date(from: date),
		      let target = Calendar.current.date(byAdding: .day, value: offset, to: shown)
		else { return }
		reload(date: DayText.string(target))
	}

	private static let timeFormatter: DateFormatter = {
		let formatter = DateFormatter()
		formatter.locale = Locale(identifier: "en_US_POSIX")
		formatter.calendar = Calendar(identifier: .gregorian)
		formatter.timeZone = .autoupdatingCurrent
		formatter.dateFormat = "HH:mm"
		return formatter
	}()
}
