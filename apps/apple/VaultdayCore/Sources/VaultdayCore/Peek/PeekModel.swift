import Foundation
import Observation

/// What a peek looks up: a task, a story, a day-log line or a calendar block.
public enum PeekSubject: Hashable, Sendable {
	case task(String)
	case story(String)
	case logLine(DayLogLine)
	/// A block of the calendar: the event or the time box of a task.
	case block(title: String, start: String, end: String, source: String, taskId: String?)
}

/// What the peek popover shows. Read-only: the record's fields, a note
/// excerpt, the last log lines and the actions the row offers.
public struct PeekContent: Hashable, Sendable {
	public struct Field: Hashable, Sendable {
		public let label: String
		public let value: String
	}

	/// The mono chips at the top: ids and the story number.
	public let chips: [String]
	/// "Checkout redesign · in progress".
	public let subtitle: String?
	/// "waiting on Janusz · 6d".
	public let badge: String?
	public let title: String
	public let fields: [Field]
	/// The note excerpt, or the log line's text for a story with no body.
	public let excerpt: String?
	public let activity: [PreviewActivity]
	/// The task the actions work on; `nil` for a log line or a bare story.
	public let taskId: String?
	public let storyNumber: String?
	public let obsidianLink: URL?
}

/// The Peek popover (`K`, or a force click): looks one record up and holds it
/// until Esc or a click elsewhere closes it.
@MainActor
@Observable
public final class PeekModel {
	public private(set) var subject: PeekSubject?
	public private(set) var content: PeekContent?
	public private(set) var errorMessage: String?

	private var vault: WorkspaceHandle?
	private let now: () -> Date

	public init(now: @escaping () -> Date = Date.init) {
		self.now = now
	}

	public func attach(to workspace: WorkspaceModel) {
		vault = workspace.vault
		dismiss()
	}

	/// Opens the workspace at `path` without a grant (tests, and hosts outside the sandbox).
	public func attach(workspacePath path: String) {
		vault = try? openWorkspace(path: path)
		dismiss()
	}

	/// Looks `subject` up and shows it.
	public func show(_ subject: PeekSubject) {
		let today = DayText.string(now())
		do {
			content = try Self.content(for: subject, in: vault, today: today)
			self.subject = subject
			errorMessage = nil
		} catch {
			content = nil
			self.subject = nil
			errorMessage = coreErrorMessage(error)
		}
	}

	/// The `obsidian://` link of `subject`'s note, without showing it (`gx`).
	public func link(for subject: PeekSubject) -> URL? {
		(try? Self.content(for: subject, in: vault, today: DayText.string(now())))?.obsidianLink
	}

	/// Shows the subject, or closes the peek when it is already showing it (`K` twice).
	public func toggle(_ subject: PeekSubject) {
		if self.subject == subject { dismiss() } else { show(subject) }
	}

	public func dismiss() {
		subject = nil
		content = nil
		errorMessage = nil
	}

	// MARK: Building

	static func content(for subject: PeekSubject, in vault: WorkspaceHandle?, today: String) throws -> PeekContent {
		switch subject {
		case let .task(id):
			guard let vault else { throw PeekError.noWorkspace }
			return task(try taskPreview(workspace: vault, id: id, today: today), today: today)
		case let .story(number):
			guard let vault else { throw PeekError.noWorkspace }
			return story(try storyPreview(workspace: vault, number: number, today: today), today: today)
		case let .logLine(line):
			return logLine(line)
		case let .block(title, start, end, source, taskId):
			return block(title: title, start: start, end: end, source: source, taskId: taskId)
		}
	}

	static func task(_ view: TaskPreviewView, today: String) -> PeekContent {
		let task = view.task
		let story = view.story
		var fields: [PeekContent.Field] = []
		if let section = task.section {
			fields.append(.init(label: "Section", value: "## " + (section == "pending-input" ? "Pending Input" : "Next Steps")))
		}
		if let created = task.created {
			fields.append(.init(label: "Created", value: PreviewMapping.shortDay(created)))
		}
		if let scheduled = task.scheduled {
			fields.append(.init(label: "Scheduled", value: BrowseDateText.scheduled(scheduled, today: today)))
		}
		if let due = task.due {
			fields.append(.init(label: "Due", value: BrowseDateText.due(due)))
		}
		if let estimate = task.estimate {
			fields.append(.init(label: "Estimate", value: DurationParsing.minutes(estimate).map(DurationText.minutes) ?? estimate))
		}
		if let assignee = task.assignedTo {
			fields.append(.init(label: "Assigned to", value: assignee))
		}
		if !view.subtasks.isEmpty {
			let done = view.subtasks.filter { $0.state == .done }.count
			fields.append(.init(label: "Subtasks", value: "\(done) of \(view.subtasks.count) done"))
		}
		let waiting = task.status == "open" || task.status == "in-progress" ? waitingBadge(task, today: today) : nil
		let own = view.taskActivity.map { PreviewMapping.activity($0, today: today) }
		let storyLines = (story?.activity ?? []).map { PreviewMapping.activity($0, today: today) }
		return PeekContent(
			chips: [task.id, task.storyNumber].compactMap(\.self),
			subtitle: story.flatMap { story in
				[story.title, story.status.map { $0.replacingOccurrences(of: "-", with: " ") }].compactMap(\.self).joined(separator: " · ")
			},
			badge: waiting,
			title: task.text,
			fields: fields,
			excerpt: nil,
			activity: Array((own.isEmpty ? storyLines : own).prefix(3)),
			taskId: task.id,
			storyNumber: task.storyNumber,
			obsidianLink: URL(string: view.obsidianLink),
		)
	}

	static func story(_ view: StoryPreviewView, today: String) -> PeekContent {
		var fields: [PeekContent.Field] = []
		if let status = view.status {
			fields.append(.init(label: "Status", value: status.replacingOccurrences(of: "-", with: " ")))
		}
		if let priority = view.priority {
			fields.append(.init(label: "Priority", value: priority))
		}
		if let due = view.due {
			fields.append(.init(label: "Due", value: BrowseDateText.due(due)))
		}
		if let estimate = view.estimate {
			fields.append(.init(label: "Estimate", value: estimate))
		}
		if !view.waiting.isEmpty {
			fields.append(.init(label: "Waiting on", value: view.waiting.map(\.assignedTo).joined(separator: ", ")))
		}
		let firstParagraph = view.blocks.lazy.compactMap { block -> String? in
			if case let .paragraph(text) = block { return text }
			return nil
		}.first
		return PeekContent(
			chips: [view.number].compactMap(\.self),
			subtitle: nil,
			badge: nil,
			title: view.title ?? view.number ?? view.relativePath,
			fields: fields,
			excerpt: firstParagraph,
			activity: view.activity.prefix(3).map { PreviewMapping.activity($0, today: today) },
			taskId: nil,
			storyNumber: view.number,
			obsidianLink: URL(string: view.obsidianLink),
		)
	}

	static func logLine(_ line: DayLogLine) -> PeekContent {
		var fields: [PeekContent.Field] = [.init(label: "Time", value: line.time)]
		if let kind = line.kind {
			fields.append(.init(label: "Kind", value: kind))
		}
		if let story = line.story {
			fields.append(.init(label: "Story", value: story))
		}
		if let task = line.task {
			fields.append(.init(label: "Task", value: task))
		}
		fields.append(.init(label: "Written by", value: line.agent ?? "you or the app"))
		return PeekContent(
			chips: [line.story, line.task].compactMap(\.self),
			subtitle: line.kind,
			badge: nil,
			title: line.text,
			fields: fields,
			excerpt: nil,
			activity: [],
			taskId: line.task,
			storyNumber: line.story,
			obsidianLink: nil,
		)
	}

	static func block(title: String, start: String, end: String, source: String, taskId: String?) -> PeekContent {
		PeekContent(
			chips: [taskId].compactMap(\.self),
			subtitle: source,
			badge: nil,
			title: title,
			fields: [.init(label: "From", value: start), .init(label: "To", value: end)],
			excerpt: nil,
			activity: [],
			taskId: taskId,
			storyNumber: nil,
			obsidianLink: nil,
		)
	}

	private static func waitingBadge(_ task: TaskDetailView, today: String) -> String? {
		guard let who = task.assignedTo?.trimmingCharacters(in: CharacterSet(charactersIn: "@ ")),
		      !who.isEmpty, who.caseInsensitiveCompare("me") != .orderedSame
		else { return nil }
		let days = task.created.flatMap { DayText.date(from: $0) }.flatMap { created in
			DayText.date(from: today).flatMap { Calendar(identifier: .gregorian).dateComponents([.day], from: created, to: $0).day }
		}
		return days.map { "waiting on \(who) · \($0)d" } ?? "waiting on \(who)"
	}
}

private enum PeekError: LocalizedError {
	case noWorkspace

	var errorDescription: String? { "No workspace is open" }
}
