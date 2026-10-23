import Foundation

/// A subtask line inside its parent's card, as the views see it.
public struct PlanSubtask: Identifiable, Hashable, Sendable {
	public let id: String
	/// The `[id:: …]` field, when the line has one.
	public let taskId: String?
	public let text: String
	public let isDone: Bool
	public let estimateMinutes: Int?
}

/// One task card of the day's list, as the views see it.
public struct PlanItem: Identifiable, Hashable, Sendable {
	public enum State: Hashable, Sendable {
		case open
		case inProgress
		case done
	}

	/// Unique on the screen: the task id, prefixed by the workspace in the All view.
	public let id: String
	public let taskId: String
	public let text: String
	public let storyNumber: String?
	public let storyTitle: String?
	public let isPendingInput: Bool
	public let state: State
	public let estimateMinutes: Int?
	public let assignedTo: String?
	public let waitingDays: Int?
	public let subtasks: [PlanSubtask]
	/// Where the scheduler put the task today, when it holds a box.
	public var slot: PlanSlot?

	public var isDone: Bool { state == .done }
}

/// A time box the scheduler laid for a task.
public struct PlanSlot: Hashable, Sendable {
	public let start: DayTime
	public let minutes: Int
	public let isLocked: Bool
}

/// The day's numbers, the three kinds of time kept apart: what was
/// estimated, what the timer counted, and what of that went to a story.
public struct DayTotals: Hashable, Sendable {
	public var plannedMinutes = 0
	public var spentMinutes = 0
	public var loggedMinutes = 0
	/// Free time left in the working day, `nil` until known.
	public var freeMinutes: Int?
	public var doneCount = 0
	public var taskCount = 0
	public var unestimatedCount = 0

	public init() {}

	/// Unfinished work beyond the free time, `0` when it fits.
	public func overFreeTime(remainingMinutes: Int) -> Int {
		guard let freeMinutes else { return 0 }
		return max(0, remainingMinutes - freeMinutes)
	}
}

/// Short durations as the design system writes them: `45m`, `2h`, `1h 30m`.
public enum DurationText {
	public static func minutes(_ minutes: Int) -> String {
		let hours = minutes / 60
		let rest = minutes % 60
		switch (hours, rest) {
		case (0, _): return "\(rest)m"
		case (_, 0): return "\(hours)h"
		default: return "\(hours)h \(rest)m"
		}
	}

	/// A running timer: `0:47:12`.
	public static func clock(seconds: Int) -> String {
		let clamped = max(0, seconds)
		return String(format: "%d:%02d:%02d", clamped / 3600, clamped / 60 % 60, clamped % 60)
	}
}

extension PlanItem {
	init(_ view: PlanItemView, idPrefix: String = "") {
		let state: State = switch view.state {
		case .open: .open
		case .inProgress: .inProgress
		case .done: .done
		}
		self.init(
			id: idPrefix + view.id,
			taskId: view.id,
			text: view.text,
			storyNumber: view.storyNumber,
			storyTitle: view.storyTitle,
			isPendingInput: view.isPendingInput,
			state: state,
			estimateMinutes: view.estimateMinutes.map(Int.init),
			assignedTo: view.assignedTo,
			waitingDays: view.waitingDays.map(Int.init),
			subtasks: view.subtasks.enumerated().map { index, subtask in
				PlanSubtask(
					id: subtask.id ?? "\(view.id)#\(index)",
					taskId: subtask.id,
					text: subtask.text,
					isDone: subtask.isDone,
					estimateMinutes: subtask.estimateMinutes.map(Int.init),
				)
			},
			slot: nil,
		)
	}
}
