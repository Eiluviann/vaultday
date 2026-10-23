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

/// The day's plan: the Today list read from the vault through the core, and
/// the order the workspace's app state holds for the day. Keeps plain Swift
/// values for the views; FFI records never leave the package.
@MainActor
@Observable
public final class DayPlanModel {
	public private(set) var workspaceName = ""
	public private(set) var date = ""
	public private(set) var stories: [TodayStory] = []
	public private(set) var log: [TodayLogLine] = []
	/// Task ids in the day's planned order, from the synced app state.
	public private(set) var plannedTaskIds: [String] = []
	public private(set) var errorMessage: String?
	/// Wall time of the last `today()` call through the bridge, in milliseconds.
	public private(set) var lastCallMilliseconds: Double?

	public init() {}

	public var openTaskCount: Int {
		stories.reduce(0) { $0 + $1.nextSteps.count + $1.pendingInput.count }
	}

	/// Loads the open workspace's view for `date` (`YYYY-MM-DD`; `nil` is the local day).
	public func load(from workspace: WorkspaceModel, date: String? = nil) {
		guard let vault = workspace.vault else {
			clear()
			return
		}
		load(vault, date: date)
		if let state = workspace.state {
			plannedTaskIds = (try? state.dayOrder(date: self.date)) ?? []
		}
	}

	/// Opens the workspace at `path` without a grant (tests, and hosts
	/// outside the sandbox) and loads the view for `date`.
	public func load(workspacePath path: String, date: String? = nil) {
		do {
			load(try openWorkspace(path: path), date: date)
		} catch {
			errorMessage = coreErrorMessage(error)
		}
	}

	private func load(_ workspace: WorkspaceHandle, date: String?) {
		do {
			let start = ContinuousClock.now
			let view = try today(workspace: workspace, date: date)
			let elapsed = ContinuousClock.now - start
			lastCallMilliseconds = Double(elapsed.components.attoseconds) / 1e15
				+ Double(elapsed.components.seconds) * 1000
			apply(view)
			errorMessage = nil
		} catch {
			errorMessage = coreErrorMessage(error)
		}
	}

	private func clear() {
		workspaceName = ""
		date = ""
		stories = []
		log = []
		plannedTaskIds = []
		errorMessage = nil
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
