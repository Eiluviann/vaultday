import Foundation
import Observation

/// An open task line, as the views see it.
public struct TodayTask: Identifiable, Hashable, Sendable {
	public let id: String
	public let taskId: String?
	public let text: String
	public let isInProgress: Bool
	public let depth: Int
	public let assignedTo: String?
	public let due: String?
	public let estimate: String?
}

/// One story's open work, as the views see it.
public struct TodayStory: Identifiable, Hashable, Sendable {
	public let id: String
	public let number: String?
	public let title: String
	public let status: String?
	public let nextSteps: [TodayTask]
	public let pendingInput: [TodayTask]
}

/// One line of the day log, as the views see it.
public struct TodayLogLine: Identifiable, Hashable, Sendable {
	public let id: String
	public let time: String
	public let kind: String?
	public let text: String
	public let story: String?
	public let task: String?
}

/// The Today screen's state: loads a workspace through the core and keeps
/// plain Swift values for the views. FFI records never leave this file.
@MainActor
@Observable
public final class TodayModel {
	public private(set) var workspaceName = ""
	public private(set) var date = ""
	public private(set) var stories: [TodayStory] = []
	public private(set) var log: [TodayLogLine] = []
	public private(set) var errorMessage: String?
	/// Wall time of the last `today()` call through the bridge, in milliseconds.
	public private(set) var lastCallMilliseconds: Double?

	public init() {}

	public var openTaskCount: Int {
		stories.reduce(0) { $0 + $1.nextSteps.count + $1.pendingInput.count }
	}

	/// Opens the workspace at `path` and loads the view for `date`
	/// (`YYYY-MM-DD`; `nil` is the local day).
	public func load(workspacePath path: String, date: String? = nil) {
		do {
			let workspace = try openWorkspace(path: path)
			let start = ContinuousClock.now
			let view = try today(workspace: workspace, date: date)
			let elapsed = ContinuousClock.now - start
			lastCallMilliseconds = Double(elapsed.components.attoseconds) / 1e15
				+ Double(elapsed.components.seconds) * 1000
			apply(view)
			errorMessage = nil
		} catch let error as CoreError {
			errorMessage = describe(error)
		} catch {
			errorMessage = String(describing: error)
		}
	}

	private func apply(_ view: TodayView) {
		workspaceName = view.workspaceName
		date = view.date
		stories = view.stories.map(mapStory)
		log = view.log.enumerated().map { index, line in
			TodayLogLine(
				id: "\(index)-\(line.time)",
				time: line.time,
				kind: line.kind,
				text: line.text,
				story: line.story,
				task: line.task,
			)
		}
	}
}

private func mapStory(_ story: StoryView) -> TodayStory {
	TodayStory(
		id: story.relativePath,
		number: story.number,
		title: story.title ?? story.number ?? story.relativePath,
		status: story.status,
		nextSteps: mapTasks(story.nextSteps, in: story.relativePath, section: "next"),
		pendingInput: mapTasks(story.pendingInput, in: story.relativePath, section: "pending"),
	)
}

private func mapTasks(_ tasks: [TaskView], in path: String, section: String) -> [TodayTask] {
	tasks.enumerated().map { index, task in
		TodayTask(
			id: task.id ?? "\(path)#\(section)#\(index)",
			taskId: task.id,
			text: task.text,
			isInProgress: task.state == .inProgress,
			depth: Int(task.depth),
			assignedTo: task.assignedTo,
			due: task.due,
			estimate: task.estimate,
		)
	}
}

private func describe(_ error: CoreError) -> String {
	switch error {
	case let .NotAWorkspace(message), let .InvalidSettings(message), let .FileAccess(message),
		let .InvalidDate(message), let .InvalidInput(message), let .NotFound(message),
		let .Conflict(message):
		message
	}
}
