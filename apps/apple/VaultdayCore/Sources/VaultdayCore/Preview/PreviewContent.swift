import Foundation

/// One block of the story note, as the preview draws it.
public enum PreviewBlock: Hashable, Sendable {
	case heading(level: Int, text: String)
	case paragraph(String)
	case bullet(depth: Int, text: String, state: BrowseRow.State?)
	case code(String)
}

/// A subtask in the preview, as a line with a checkbox.
public struct PreviewSubtask: Identifiable, Hashable, Sendable {
	public let id: String
	public let taskId: String?
	public let text: String
	public let state: BrowseRow.State
	public let estimateMinutes: Int?

	public var estimateLabel: String? { estimateMinutes.map(DurationText.minutes) }
}

/// A task of the story that waits on someone.
public struct PreviewWaiting: Identifiable, Hashable, Sendable {
	public var id: String { taskId ?? text }
	public let taskId: String?
	public let text: String
	public let assignedTo: String
	public let ageDays: Int?

	/// "Maya · 6d".
	public var detail: String {
		ageDays.map { "\(assignedTo) · \($0)d" } ?? assignedTo
	}
}

/// One day-log line about the task or its story.
public struct PreviewActivity: Identifiable, Hashable, Sendable {
	public let id: String
	/// "09:30" for a line of today, else "14 Oct 16:42".
	public let whenLabel: String
	public let kind: String?
	public let text: String
	/// The model that wrote the line, when one did.
	public let agent: String?
}

/// Everything the task preview slide-over shows: the story header, the
/// story note read-only, the subtasks, the waiting items and the recent
/// activity. Plain values; the view draws them and nothing else.
public struct TaskPreviewContent: Hashable, Sendable {
	public let taskId: String
	public let text: String
	public let state: BrowseRow.State
	public let storyNumber: String?
	public let storyTitle: String?
	public let storyStatus: String?
	public let sectionTitle: String?
	public let estimateLabel: String?
	public let scheduledLabel: String?
	public let dueLabel: String?
	public let assignedTo: String?
	/// The story note without its title, frontmatter, markers and task sections.
	public let blocks: [PreviewBlock]
	public let subtasks: [PreviewSubtask]
	public let waiting: [PreviewWaiting]
	public let activity: [PreviewActivity]
	public let obsidianLink: URL?
	/// One line shown under the header when the link had to fall back (the
	/// workspace sits outside its Obsidian vault); `nil` otherwise.
	public let obsidianLinkNotice: String?

	/// "task-118 · WEB-142 · Website relaunch".
	public var headerLine: String {
		[taskId, storyNumber, storyTitle].compactMap(\.self).joined(separator: " · ")
	}

	/// "In progress" for a story, as written in the note.
	public var storyStatusLabel: String? {
		storyStatus.map { $0.replacingOccurrences(of: "-", with: " ").capitalizedFirst }
	}
}

extension String {
	var capitalizedFirst: String {
		prefix(1).uppercased() + dropFirst()
	}
}

/// How the preview and the peek write what the core read.
enum PreviewMapping {
	static func state(_ state: BrowseRowState) -> BrowseRow.State {
		switch state {
		case .open: .open
		case .inProgress: .inProgress
		case .done: .done
		case .cancelled: .cancelled
		}
	}

	static func block(_ block: NoteBlockView) -> PreviewBlock {
		switch block {
		case let .heading(level, text): .heading(level: Int(level), text: text)
		case let .paragraph(text): .paragraph(text)
		case let .bullet(depth, text, state): .bullet(depth: Int(depth), text: text, state: state.map(Self.state))
		case let .code(text): .code(text)
		}
	}

	static func activity(_ entry: ActivityEntryView, today: String) -> PreviewActivity {
		let whenLabel = entry.date == today ? entry.time : "\(shortDay(entry.date)) \(entry.time)"
		return PreviewActivity(
			id: "\(entry.date) \(entry.time) \(entry.text)",
			whenLabel: whenLabel,
			kind: entry.kind,
			text: DayLogText.display(entry.text, kind: entry.kind),
			agent: entry.agent,
		)
	}

	/// "14 Oct".
	static func shortDay(_ iso: String) -> String {
		guard let day = DayText.date(from: iso) else { return iso }
		return dayMonth.string(from: day)
	}

	private static let dayMonth: DateFormatter = {
		let formatter = DateFormatter()
		formatter.locale = Locale(identifier: "en_US_POSIX")
		formatter.calendar = Calendar(identifier: .gregorian)
		formatter.timeZone = .autoupdatingCurrent
		formatter.dateFormat = "d MMM"
		return formatter
	}()

	static func content(_ view: TaskPreviewView, today: String) -> TaskPreviewContent {
		let task = view.task
		let story = view.story
		let recent = (story?.activity ?? []).map { activity($0, today: today) }
		return TaskPreviewContent(
			taskId: task.id ?? "",
			text: task.text,
			state: taskState(task.status),
			storyNumber: story?.number ?? task.storyNumber,
			storyTitle: story?.title,
			storyStatus: story?.status,
			sectionTitle: task.section.map { $0 == "pending-input" ? "Pending Input" : "Next Steps" },
			estimateLabel: task.estimate.flatMap(estimateLabel),
			scheduledLabel: task.scheduled.map { BrowseDateText.scheduled($0, today: today) },
			dueLabel: task.due.map(BrowseDateText.due),
			assignedTo: task.assignedTo,
			blocks: (story?.blocks ?? []).map(block),
			subtasks: view.subtasks.enumerated().map { index, subtask in
				PreviewSubtask(
					id: subtask.id ?? "\(task.id ?? "")#\(index)",
					taskId: subtask.id,
					text: subtask.text,
					state: state(subtask.state),
					estimateMinutes: subtask.estimateMinutes.map(Int.init),
				)
			},
			waiting: (story?.waiting ?? []).map {
				PreviewWaiting(taskId: $0.taskId, text: $0.text, assignedTo: $0.assignedTo, ageDays: $0.ageDays.map(Int.init))
			},
			activity: recent,
			obsidianLink: URL(string: view.obsidianLink),
			obsidianLinkNotice: view.obsidianLinkNotice,
		)
	}

	/// The task detail's status word.
	static func taskState(_ status: String) -> BrowseRow.State {
		switch status {
		case "in-progress": .inProgress
		case "done": .done
		case "cancelled": .cancelled
		default: .open
		}
	}

	/// A task's `[estimate:: …]` as the design writes it (`1h 30m`); the text as written when it is not a length.
	private static func estimateLabel(_ written: String) -> String? {
		DurationParsing.minutes(written).map(DurationText.minutes) ?? written
	}
}

/// Reads the lengths task lines carry: `30m`, `2h`, `1h30`, `1.5h`.
enum DurationParsing {
	static func minutes(_ text: String) -> Int? {
		parseEstimateMinutes(text: text).map(Int.init)
	}
}
